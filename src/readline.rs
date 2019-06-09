use std::io::Write;

#[derive(Debug)]
pub enum Error {
    EOF,
    IOError(std::io::Error),
}

pub struct Readline {
    reader: Option<KeyReader>,
    state: ReadlineState,
    _raw_screen: crossterm::RawScreen,
}

struct ReadlineState {
    prompt: String,
    echo: bool,

    buffer: String,
    cursor: usize,
    wrote_prompt: bool,
}

impl Readline {
    fn new(prompt: &str, echo: bool) -> Self {
        let screen = crossterm::RawScreen::into_raw_mode().unwrap();

        Readline {
            reader: None,
            state: ReadlineState {
                prompt: prompt.to_string(),
                echo,
                buffer: String::new(),
                cursor: 0,
                wrote_prompt: false,
            },
            _raw_screen: screen,
        }
    }
}

impl ReadlineState {
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
                self.echo_char(c).map_err(|e| Error::IOError(e))?;

                if c == '\n' {
                    return Ok(futures::Async::Ready(self.buffer.clone()));
                }
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
            }
            crossterm::KeyEvent::Ctrl(c) => {
                if c == 'd' {
                    if self.buffer.is_empty() {
                        self.echo_char('\n')
                            .map_err(|e| Error::IOError(e))?;
                        return Err(Error::EOF);
                    }
                }
                if c == 'c' {
                    self.buffer = String::new();
                    self.cursor = 0;
                    self.echo_char('\n').map_err(|e| Error::IOError(e))?;
                    self.prompt().map_err(|e| Error::IOError(e))?;
                }
            }
            crossterm::KeyEvent::Backspace => {
                if self.cursor != 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                    self.echo(b"\x08 \x08").map_err(|e| Error::IOError(e))?;
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

    fn echo(&self, bytes: &[u8]) -> std::io::Result<()> {
        let bytes: Vec<_> = bytes
            .iter()
            // replace \n with \r\n
            .fold(vec![], |mut acc, &c| {
                if c == b'\n' {
                    acc.push(b'\r');
                    acc.push(b'\n');
                } else {
                    if self.echo {
                        acc.push(c);
                    }
                }
                acc
            });
        self.write(&bytes)
    }

    fn echo_char(&self, c: char) -> std::io::Result<()> {
        let mut buf = [0u8; 4];
        self.echo(c.encode_utf8(&mut buf[..]).as_bytes())
    }
}

#[must_use = "futures do nothing unless polled"]
impl futures::future::Future for Readline {
    type Item = String;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        if !self.state.wrote_prompt {
            self.state.prompt().map_err(|e| Error::IOError(e))?;
            self.state.wrote_prompt = true;
        }

        let reader = self
            .reader
            .get_or_insert_with(|| KeyReader::new(futures::task::current()));
        while let Some(event) = reader.poll() {
            let a = self.state.process_event(event)?;
            if a.is_ready() {
                return Ok(a);
            }
        }
        Ok(futures::Async::NotReady)
    }
}

pub fn readline(prompt: &str, echo: bool) -> Readline {
    Readline::new(prompt, echo)
}

struct KeyReader {
    events: std::sync::mpsc::Receiver<crossterm::InputEvent>,
    quit: std::sync::mpsc::Sender<()>,
}

impl KeyReader {
    fn new(task: futures::task::Task) -> Self {
        let reader = crossterm::input().read_sync();
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        let (quit_tx, quit_rx) = std::sync::mpsc::channel();
        // TODO: this is pretty janky - it'd be better to build in more useful
        // support to crossterm directly
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
        let _ = self.quit.send(());
    }
}
