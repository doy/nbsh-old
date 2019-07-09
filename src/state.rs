use futures::stream::Stream as _;
use snafu::{OptionExt as _, ResultExt as _};
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("invalid command index: {}", idx))]
    InvalidCommandIndex { idx: usize },

    #[snafu(display("error sending message"))]
    Sending,

    #[snafu(display("error printing output: {}", source))]
    PrintOutput { source: std::io::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum StateEvent {
    Line(usize, String, futures::sync::oneshot::Sender<Result<()>>),
}

pub struct State {
    r: futures::sync::mpsc::Receiver<StateEvent>,
    commands: std::collections::HashMap<usize, Command>,
}

impl State {
    pub fn new(r: futures::sync::mpsc::Receiver<StateEvent>) -> Self {
        Self {
            r,
            commands: std::collections::HashMap::new(),
        }
    }

    pub fn eval(
        &mut self,
        idx: usize,
        line: &str,
        res: futures::sync::oneshot::Sender<Result<()>>,
    ) -> std::result::Result<
        (),
        (futures::sync::oneshot::Sender<Result<()>>, Error),
    > {
        if self.commands.contains_key(&idx) {
            return Err((res, Error::InvalidCommandIndex { idx }));
        }
        let eval = crate::eval::eval(line).context(Eval);
        match eval {
            Ok(eval) => {
                self.commands.insert(idx, Command::new(eval, res));
            }
            Err(e) => return Err((res, e)),
        }
        Ok(())
    }

    pub fn command_start(
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
        eprint!("running '{} {:?}'\r\n", cmd, args);
        command.cmd = Some(cmd);
        command.args = Some(args);
        Ok(())
    }

    pub fn command_output(
        &mut self,
        idx: usize,
        output: &[u8],
    ) -> Result<()> {
        let command = self
            .commands
            .get_mut(&idx)
            .context(InvalidCommandIndex { idx })?;
        command.output.append(&mut output.to_vec());

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write(output).context(PrintOutput)?;
        stdout.flush().context(PrintOutput)?;

        Ok(())
    }

    pub fn command_exit(
        &mut self,
        idx: usize,
        status: std::process::ExitStatus,
    ) -> Result<()> {
        let command = self
            .commands
            .get_mut(&idx)
            .context(InvalidCommandIndex { idx })?;
        command.status = Some(status);
        eprint!("command exited: {}\r\n", status);
        Ok(())
    }

    fn poll_with_errors(&mut self) -> futures::Poll<(), Error> {
        loop {
            let mut did_work = false;

            match self.r.poll().map_err(|()| unreachable!())? {
                futures::Async::Ready(Some(StateEvent::Line(
                    idx,
                    line,
                    res,
                ))) => {
                    match self.eval(idx, &line, res) {
                        Ok(()) => {}
                        Err((res, e)) => {
                            res.send(Err(e)).map_err(|_| Error::Sending)?;
                        }
                    }
                    did_work = true;
                }
                futures::Async::Ready(None) => {
                    return Ok(futures::Async::Ready(()));
                }
                futures::Async::NotReady => {}
            }

            for idx in self.commands.keys().cloned().collect::<Vec<usize>>() {
                match self
                    .commands
                    .get_mut(&idx)
                    .unwrap()
                    .future
                    .poll()
                    .context(Eval)?
                {
                    futures::Async::Ready(Some(event)) => match event {
                        crate::eval::CommandEvent::CommandStart(
                            cmd,
                            args,
                        ) => {
                            self.command_start(idx, &cmd, &args)?;
                            did_work = true;
                        }
                        crate::eval::CommandEvent::Output(out) => {
                            self.command_output(idx, &out)?;
                            did_work = true;
                        }
                        crate::eval::CommandEvent::CommandExit(status) => {
                            self.command_exit(idx, status)?;
                            did_work = true;
                        }
                    },
                    futures::Async::Ready(None) => {
                        self.commands
                            .remove(&idx)
                            .context(InvalidCommandIndex { idx })?
                            .res
                            .send(Ok(()))
                            .map_err(|_| Error::Sending)?;
                        did_work = true;
                    }
                    futures::Async::NotReady => {}
                }
            }

            if !did_work {
                return Ok(futures::Async::NotReady);
            }
        }
    }
}

impl futures::future::Future for State {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        loop {
            match self.poll_with_errors() {
                Ok(a) => return Ok(a),
                Err(e) => {
                    eprint!("error polling state: {}\r\n", e);
                }
            }
        }
    }
}

struct Command {
    future: crate::eval::Eval,
    res: futures::sync::oneshot::Sender<Result<()>>,
    cmd: Option<String>,
    args: Option<Vec<String>>,
    output: Vec<u8>,
    status: Option<std::process::ExitStatus>,
}

impl Command {
    fn new(
        future: crate::eval::Eval,
        res: futures::sync::oneshot::Sender<Result<()>>,
    ) -> Self {
        Self {
            future,
            res,
            cmd: None,
            args: None,
            output: vec![],
            status: None,
        }
    }
}
