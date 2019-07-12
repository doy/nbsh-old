use futures::future::Future as _;
use futures::stream::Stream as _;
use snafu::{OptionExt as _, ResultExt as _};
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("invalid command index: {}", idx))]
    InvalidCommandIndex { idx: usize },

    #[snafu(display(
        "failed to put the terminal into raw mode: {}",
        source
    ))]
    IntoRawMode { source: std::io::Error },

    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: std::io::Error },

    #[snafu(display("eof"))]
    EOF,
}

type Result<T> = std::result::Result<T, Error>;

pub fn tui() {
    tokio::run(Tui::new());
}

#[derive(Default)]
pub struct Tui {
    idx: usize,
    readline: Option<crate::readline::Readline>,
    commands: std::collections::HashMap<usize, Command>,
    raw_screen: Option<crossterm::RawScreen>,
}

impl Tui {
    pub fn new() -> Self {
        Self::default()
    }

    fn read() -> crate::readline::Readline {
        crate::readline::Readline::new().set_raw(false)
    }

    fn eval(
        &mut self,
        idx: usize,
        line: &str,
    ) -> std::result::Result<(), Error> {
        if self.commands.contains_key(&idx) {
            return Err(Error::InvalidCommandIndex { idx });
        }
        let eval = crate::eval::Eval::new(line).set_raw(false);
        self.commands.insert(idx, Command::new(eval));
        Ok(())
    }

    fn print(
        &mut self,
        idx: usize,
        event: crate::eval::CommandEvent,
    ) -> Result<()> {
        match event {
            crate::eval::CommandEvent::CommandStart(cmd, args) => {
                self.command_start(idx, &cmd, &args)
            }
            crate::eval::CommandEvent::Output(out) => {
                self.command_output(idx, &out)
            }
            crate::eval::CommandEvent::CommandExit(status) => {
                self.command_exit(idx, status)
            }
        }
    }

    fn command_start(
        &mut self,
        idx: usize,
        cmd: &str,
        args: &[String],
    ) -> Result<()> {
        let command = self
            .commands
            .get_mut(&idx)
            .context(InvalidCommandIndex { idx })?;
        let cmd = cmd.to_string();
        let args = args.to_vec();
        command.cmd = Some(cmd);
        command.args = Some(args);
        Ok(())
    }

    fn command_output(&mut self, idx: usize, output: &[u8]) -> Result<()> {
        let command = self
            .commands
            .get_mut(&idx)
            .context(InvalidCommandIndex { idx })?;
        command.output.append(&mut output.to_vec());

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write(output).context(Print)?;
        stdout.flush().context(Print)?;

        Ok(())
    }

    fn command_exit(
        &mut self,
        idx: usize,
        status: std::process::ExitStatus,
    ) -> Result<()> {
        let command = self
            .commands
            .get_mut(&idx)
            .context(InvalidCommandIndex { idx })?;
        command.status = Some(status);
        Ok(())
    }

    fn poll_read(&mut self) {
        if self.readline.is_none() && self.commands.is_empty() {
            self.idx += 1;
            self.readline = Some(Self::read())
        }
    }

    fn poll_eval(&mut self) -> Result<bool> {
        if let Some(mut r) = self.readline.take() {
            match r.poll() {
                Ok(futures::Async::Ready(line)) => {
                    match self.eval(self.idx, &line) {
                        Ok(())
                        | Err(Error::Eval {
                            source:
                                crate::eval::Error::Parser {
                                    source:
                                        crate::parser::Error::CommandRequired,
                                    ..
                                },
                        }) => {}
                        Err(e) => return Err(e),
                    }
                    Ok(true)
                }
                Ok(futures::Async::NotReady) => {
                    self.readline.replace(r);
                    Ok(false)
                }
                Err(crate::readline::Error::EOF) => Err(Error::EOF),
                Err(e) => Err(e).context(Read),
            }
        } else {
            Ok(false)
        }
    }

    fn poll_print(&mut self) -> Result<bool> {
        let mut did_work = false;

        for idx in self.commands.keys().cloned().collect::<Vec<usize>>() {
            match self.commands.get_mut(&idx).unwrap().future.poll() {
                Ok(futures::Async::Ready(Some(event))) => {
                    self.print(idx, event)?;
                    did_work = true;
                }
                Ok(futures::Async::Ready(None)) => {
                    self.commands
                        .remove(&idx)
                        .context(InvalidCommandIndex { idx })?;
                    did_work = true;
                }
                Ok(futures::Async::NotReady) => {}

                // Parser and Command errors are always fatal, but execution
                // errors might not be
                Err(e @ crate::eval::Error::Parser { .. }) => {
                    self.commands
                        .remove(&idx)
                        .context(InvalidCommandIndex { idx })?;
                    return Err(e).context(Eval);
                }
                Err(e @ crate::eval::Error::Command { .. }) => {
                    self.commands
                        .remove(&idx)
                        .context(InvalidCommandIndex { idx })?;
                    return Err(e).context(Eval);
                }
                Err(e) => {
                    return Err(e).context(Eval);
                }
            }
        }

        Ok(did_work)
    }

    fn poll_with_errors(&mut self) -> futures::Poll<(), Error> {
        if self.raw_screen.is_none() {
            self.raw_screen = Some(
                crossterm::RawScreen::into_raw_mode().context(IntoRawMode)?,
            );
        }

        loop {
            let mut did_work = false;

            self.poll_read();
            did_work |= self.poll_eval()?;
            did_work |= self.poll_print()?;

            if !did_work {
                return Ok(futures::Async::NotReady);
            }
        }
    }
}

impl futures::future::Future for Tui {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        loop {
            match self.poll_with_errors() {
                Ok(a) => return Ok(a),
                Err(Error::EOF) => return Ok(futures::Async::Ready(())),
                Err(e) => {
                    eprint!("error polling state: {}\r\n", e);
                }
            }
        }
    }
}

struct Command {
    future: crate::eval::Eval,
    cmd: Option<String>,
    args: Option<Vec<String>>,
    output: Vec<u8>,
    status: Option<std::process::ExitStatus>,
}

impl Command {
    fn new(future: crate::eval::Eval) -> Self {
        Self {
            future,
            cmd: None,
            args: None,
            output: vec![],
            status: None,
        }
    }
}
