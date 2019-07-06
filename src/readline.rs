use snafu::ResultExt as _;
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("failed to write to the terminal: {}", source))]
    WriteToTerminal { source: std::io::Error },

    #[snafu(display("end of input"))]
    EOF,

    #[snafu(display(
        "failed to put the terminal into raw mode: {}",
        source
    ))]
    IntoRawMode { source: std::io::Error },

    #[snafu(display(
        "failed to spawn a background thread to read terminal input: {}",
        source
    ))]
    TerminalInputReadingThread { source: std::io::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn readline(prompt: &str, echo: bool) -> Result<Readline> {
    Readline::new(prompt, echo)
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
    fn new(prompt: &str, echo: bool) -> Result<Self> {
        let screen =
            crossterm::RawScreen::into_raw_mode().context(IntoRawMode)?;

        Ok(Self {
            reader: None,
            state: ReadlineState {
                prompt: prompt.to_string(),
                echo,
                buffer: String::new(),
                cursor: 0,
                wrote_prompt: false,
            },
            _raw_screen: screen,
        })
    }

    fn with_reader<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&KeyReader, &mut ReadlineState) -> Result<T>,
    {
        let mut reader_opt = self.reader.take();
        if reader_opt.is_none() {
            reader_opt = Some(KeyReader::new(futures::task::current())?);
        }
        let ret = f(reader_opt.as_ref().unwrap(), &mut self.state);
        self.reader = reader_opt;
        ret
    }
}

impl ReadlineState {
    fn process_event(
        &mut self,
        event: crossterm::InputEvent,
    ) -> std::result::Result<futures::Async<String>, Error> {
        match event {
            crossterm::InputEvent::Keyboard(e) => {
                return self.process_keyboard_event(&e);
            }
            _ => {}
        }

        Ok(futures::Async::NotReady)
    }

    fn process_keyboard_event(
        &mut self,
        event: &crossterm::KeyEvent,
    ) -> std::result::Result<futures::Async<String>, Error> {
        match *event {
            crossterm::KeyEvent::Char('\n') => {
                self.echo_char('\n').context(WriteToTerminal)?;
                return Ok(futures::Async::Ready(self.buffer.clone()));
            }
            crossterm::KeyEvent::Char('\t') => {
                // TODO
            }
            crossterm::KeyEvent::Char(c) => {
                if self.cursor != self.buffer.len() {
                    self.echo(b"\x1b[@").context(WriteToTerminal)?;
                }
                self.echo_char(c).context(WriteToTerminal)?;
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
            }
            crossterm::KeyEvent::Ctrl(c) => match c {
                'a' => {
                    if self.cursor != 0 {
                        self.echo(
                            &format!("\x1b[{}D", self.cursor).into_bytes(),
                        )
                        .context(WriteToTerminal)?;
                        self.cursor = 0;
                    }
                }
                'c' => {
                    self.buffer = String::new();
                    self.cursor = 0;
                    self.echo_char('\n').context(WriteToTerminal)?;
                    self.prompt().context(WriteToTerminal)?;
                }
                'd' => {
                    if self.buffer.is_empty() {
                        self.echo_char('\n').context(WriteToTerminal)?;
                        return EOF.fail();
                    }
                }
                'e' => {
                    if self.cursor != self.buffer.len() {
                        self.echo(
                            &format!(
                                "\x1b[{}C",
                                self.buffer.len() - self.cursor
                            )
                            .into_bytes(),
                        )
                        .context(WriteToTerminal)?;
                        self.cursor = self.buffer.len();
                    }
                }
                'u' => {
                    if self.cursor != 0 {
                        self.echo(
                            std::iter::repeat(b'\x08')
                                .take(self.cursor)
                                .chain(
                                    format!("\x1b[{}P", self.cursor)
                                        .into_bytes(),
                                )
                                .collect::<Vec<_>>()
                                .as_ref(),
                        )
                        .context(WriteToTerminal)?;
                        self.buffer = self.buffer.split_off(self.cursor);
                        self.cursor = 0;
                    }
                }
                _ => {}
            },
            crossterm::KeyEvent::Backspace => {
                if self.cursor != 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                    if self.cursor == self.buffer.len() {
                        self.echo(b"\x08 \x08").context(WriteToTerminal)?;
                    } else {
                        self.echo(b"\x08\x1b[P").context(WriteToTerminal)?;
                    }
                }
            }
            crossterm::KeyEvent::Left => {
                if self.cursor != 0 {
                    self.cursor -= 1;
                    self.write(b"\x1b[D").context(WriteToTerminal)?;
                }
            }
            crossterm::KeyEvent::Right => {
                if self.cursor != self.buffer.len() {
                    self.cursor += 1;
                    self.write(b"\x1b[C").context(WriteToTerminal)?;
                }
            }
            crossterm::KeyEvent::Delete => {
                if self.cursor != self.buffer.len() {
                    self.buffer.remove(self.cursor);
                    self.echo(b"\x1b[P").context(WriteToTerminal)?;
                }
            }
            _ => {}
        }

        Ok(futures::Async::NotReady)
    }

    fn write(&self, buf: &[u8]) -> std::io::Result<()> {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(buf)?;
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
        let mut buf = [0_u8; 4];
        self.echo(c.encode_utf8(&mut buf[..]).as_bytes())
    }
}

#[must_use = "futures do nothing unless polled"]
impl futures::future::Future for Readline {
    type Item = String;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        if !self.state.wrote_prompt {
            self.state.prompt().context(WriteToTerminal)?;
            self.state.wrote_prompt = true;
        }

        self.with_reader(|reader, state| {
            loop {
                match reader.events.try_recv() {
                    Ok(event) => {
                        let a = state.process_event(event)?;
                        if a.is_ready() {
                            return Ok(a);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return Ok(futures::Async::NotReady);
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // is EOF correct here?
                        return EOF.fail();
                    }
                }
            }
        })
    }
}

struct KeyReader {
    events: std::sync::mpsc::Receiver<crossterm::InputEvent>,
    quit: std::sync::mpsc::Sender<()>,
}

impl KeyReader {
    fn new(task: futures::task::Task) -> Result<Self> {
        let reader = crossterm::input().read_sync();
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        let (quit_tx, quit_rx) = std::sync::mpsc::channel();
        // TODO: this is pretty janky - it'd be better to build in more useful
        // support to crossterm directly
        std::thread::Builder::new()
            .spawn(move || {
                for event in reader {
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

        Ok(Self {
            events: events_rx,
            quit: quit_tx,
        })
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
