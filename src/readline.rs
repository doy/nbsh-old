use std::io::Write;

#[derive(Debug)]
pub enum Error {
    EOF,
    IOError(std::io::Error),
}

pub struct Readline {
    reader: Option<KeyReader>,
    buffer: String,
    prompt: String,
    wrote_prompt: bool,
    echo: bool,
}

impl Readline {
    fn process_event(
        &mut self,
        event: crossterm::InputEvent,
    ) -> std::result::Result<futures::Async<String>, Error> {
        match event {
            crossterm::InputEvent::Keyboard(e) => {
                return self.process_keyboard_event(e)
            }
            _ => {}
        }
        return Ok(futures::Async::NotReady);
    }

    fn process_keyboard_event(
        &mut self,
        event: crossterm::KeyEvent,
    ) -> std::result::Result<futures::Async<String>, Error> {
        match event {
            crossterm::KeyEvent::Char(c) => {
                self.echo(c).map_err(|e| Error::IOError(e))?;

                if c == '\n' {
                    return Ok(futures::Async::Ready(self.buffer.clone()));
                }
                self.buffer.push(c);
            }
            crossterm::KeyEvent::Ctrl(c) => {
                if c == 'd' {
                    if self.buffer.is_empty() {
                        self.echo('\n').map_err(|e| Error::IOError(e))?;
                        return Err(Error::EOF);
                    }
                }
                if c == 'c' {
                    self.buffer = String::new();
                    self.echo('\n').map_err(|e| Error::IOError(e))?;
                    self.prompt().map_err(|e| Error::IOError(e))?;
                }
            }
            _ => {}
        }
        return Ok(futures::Async::NotReady);
    }

    fn write(&self, buf: &[u8]) -> std::io::Result<()> {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write(buf)?;
        stdout.flush()
    }

    fn prompt(&self) -> std::io::Result<()> {
        self.write(self.prompt.as_bytes())
    }

    fn echo(&self, c: char) -> std::io::Result<()> {
        if c == '\n' {
            self.write(b"\r\n")?;
            return Ok(());
        }

        if !self.echo {
            return Ok(());
        }

        let mut buf = [0u8; 4];
        self.write(c.encode_utf8(&mut buf[..]).as_bytes())
    }
}

impl futures::future::Future for Readline {
    type Item = String;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        if !self.wrote_prompt {
            self.prompt().map_err(|e| Error::IOError(e))?;
            self.wrote_prompt = true;
        }

        let reader = self.reader.get_or_insert_with(|| {
            KeyReader::new(tokio::prelude::task::current())
        });
        if let Some(event) = reader.poll() {
            self.process_event(event)
        } else {
            Ok(futures::Async::NotReady)
        }
    }
}

pub fn readline(prompt: &str, echo: bool) -> Readline {
    Readline {
        reader: None,
        buffer: String::new(),
        prompt: prompt.to_string(),
        wrote_prompt: false,
        echo,
    }
}

struct KeyReader {
    events: std::sync::mpsc::Receiver<crossterm::InputEvent>,
    quit: std::sync::mpsc::Sender<()>,
}

impl KeyReader {
    fn new(task: tokio::prelude::task::Task) -> Self {
        let reader = crossterm::input().read_sync();
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        let (quit_tx, quit_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for event in reader {
                let newline = event
                    == crossterm::InputEvent::Keyboard(
                        crossterm::KeyEvent::Char('\n'),
                    );
                events_tx.send(event).unwrap();
                task.notify();
                if newline {
                    break;
                }
                if let Ok(_) = quit_rx.try_recv() {
                    break;
                }
            }
        });

        KeyReader {
            events: events_rx,
            quit: quit_tx,
        }
    }

    fn poll(&self) -> Option<crossterm::InputEvent> {
        if let Ok(event) = self.events.try_recv() {
            return Some(event);
        }
        None
    }
}

impl Drop for KeyReader {
    fn drop(&mut self) {
        // don't care if it fails to send, this can happen if the thread
        // terminates due to seeing a newline before the keyreader goes out of
        // scope
        match self.quit.send(()) {
            _ => {}
        }
    }
}
