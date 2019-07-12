use futures::future::Future as _;
use futures::stream::Stream as _;
use snafu::{OptionExt as _, ResultExt as _};
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("invalid command index: {}", idx))]
    InvalidCommandIndex { idx: usize },

    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: std::io::Error },

    #[snafu(display("eof"))]
    EOF,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn tui() {
    tokio::run(Tui::new());
}

#[derive(Default)]
pub struct Tui {
    idx: usize,
    readline: Option<crate::readline::Readline>,
    commands: std::collections::HashMap<usize, Command>,
}

impl Tui {
    pub fn new() -> Self {
        Self::default()
    }

    fn read() -> Result<crate::readline::Readline> {
        crate::readline::readline("$ ", true).context(Read)
    }

    fn eval(
        &mut self,
        idx: usize,
        line: &str,
    ) -> std::result::Result<(), Error> {
        if self.commands.contains_key(&idx) {
            return Err(Error::InvalidCommandIndex { idx });
        }
        let eval = crate::eval::eval(line).context(Eval);
        match eval {
            Ok(eval) => {
                self.commands.insert(idx, Command::new(eval));
            }
            Err(e) => return Err(e),
        }
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

    fn poll_read(&mut self) -> Result<bool> {
        if self.readline.is_none() && self.commands.is_empty() {
            self.idx += 1;
            self.readline = Some(Self::read()?)
        }
        Ok(false)
    }

    fn poll_eval(&mut self) -> Result<bool> {
        if let Some(mut r) = self.readline.take() {
            match r.poll() {
                Ok(futures::Async::Ready(line)) => {
                    // overlapping RawScreen lifespans don't work properly
                    // - if readline creates a RawScreen, then eval
                    // creates a separate one, then readline drops it, the
                    // screen will go back to cooked even though a
                    // RawScreen instance is still live.
                    drop(r);

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
            match self
                .commands
                .get_mut(&idx)
                .unwrap()
                .future
                .poll()
                .context(Eval)?
            {
                futures::Async::Ready(Some(event)) => {
                    self.print(idx, event)?;
                    did_work = true;
                }
                futures::Async::Ready(None) => {
                    self.commands
                        .remove(&idx)
                        .context(InvalidCommandIndex { idx })?;
                    did_work = true;
                }
                futures::Async::NotReady => {}
            }
        }

        Ok(did_work)
    }

    fn poll_with_errors(&mut self) -> futures::Poll<(), Error> {
        loop {
            let mut did_work = false;

            did_work |= self.poll_read()?;
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
