use futures::sink::Sink as _;
use snafu::ResultExt as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("failed to read from event channel: {}", source))]
    ReadChannel {
        source: tokio::sync::mpsc::error::UnboundedRecvError,
    },

    #[snafu(display(
        "failed to spawn a background thread to read terminal input: {}",
        source
    ))]
    TerminalInputReadingThread { source: std::io::Error },
}

pub struct KeyReader {
    events:
        Option<tokio::sync::mpsc::UnboundedReceiver<crossterm::InputEvent>>,
    quit: Option<tokio::sync::oneshot::Sender<()>>,
}

impl KeyReader {
    pub fn new() -> Self {
        Self {
            events: None,
            quit: None,
        }
    }
}

impl futures::stream::Stream for KeyReader {
    type Item = crossterm::InputEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        if self.events.is_none() {
            let task = futures::task::current();
            let reader = crossterm::input().read_sync();
            let (events_tx, events_rx) =
                tokio::sync::mpsc::unbounded_channel();
            let mut events_tx = events_tx.wait();
            let (quit_tx, mut quit_rx) = tokio::sync::oneshot::channel();
            // TODO: this is pretty janky - it'd be better to build in more
            // useful support to crossterm directly
            std::thread::Builder::new()
                .spawn(move || {
                    for event in reader {
                        // sigh, this is extra janky, but otherwise the thread
                        // will outlive the current instance and eat the first
                        // character typed that was supposed to go to the
                        // thread spawned by the next instance
                        let newline = event
                            == crossterm::InputEvent::Keyboard(
                                crossterm::KeyEvent::Char('\n'),
                            );
                        // unwrap is unpleasant, but so is figuring out how to
                        // propagate the error back to the main thread
                        events_tx.send(event).unwrap();
                        task.notify();
                        if newline {
                            break;
                        }
                        if quit_rx.try_recv().is_ok() {
                            break;
                        }
                    }
                })
                .context(TerminalInputReadingThread)?;

            self.events = Some(events_rx);
            self.quit = Some(quit_tx);
        }

        self.events.as_mut().unwrap().poll().context(ReadChannel)
    }
}

impl Drop for KeyReader {
    fn drop(&mut self) {
        if let Some(quit_tx) = self.quit.take() {
            // don't care if it fails to send, this can happen if the thread
            // terminates due to seeing a newline before the keyreader goes
            // out of scope
            let _ = quit_tx.send(());
        }
    }
}
