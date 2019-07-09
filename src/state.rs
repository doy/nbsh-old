use futures::future::Future as _;
use futures::sink::Sink as _;
use futures::stream::Stream as _;
use snafu::{OptionExt as _, ResultExt as _};
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("invalid command index: {}", idx))]
    InvalidCommandIndex { idx: usize },

    #[snafu(display("error sending message: {}", source))]
    Send {
        source: futures::sync::mpsc::SendError<StateEvent>,
    },

    #[snafu(display("error printing output: {}", source))]
    PrintOutput { source: std::io::Error },

    #[snafu(display("this error should not be possible"))]
    Unreachable,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum StateEvent {
    Start(usize, String, Vec<String>),
    Output(usize, Vec<u8>),
    Exit(usize, std::process::ExitStatus),
}

#[derive(Debug)]
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

    pub fn command_start(
        &mut self,
        idx: usize,
        cmd: &str,
        args: &[String],
    ) -> Result<()> {
        snafu::ensure!(
            !self.commands.contains_key(&idx),
            InvalidCommandIndex { idx }
        );
        let command = Command::new(cmd, args);
        self.commands.insert(idx, command.clone());
        eprint!("running '{} {:?}'\r\n", command.cmd, command.args);
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
}

impl futures::future::Future for State {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        loop {
            let event = futures::try_ready!(self
                .r
                .poll()
                .map_err(|_| Error::Unreachable));
            match event {
                Some(StateEvent::Start(idx, cmd, args)) => {
                    self.command_start(idx, &cmd, &args)?;
                }
                Some(StateEvent::Output(idx, output)) => {
                    self.command_output(idx, &output)?;
                }
                Some(StateEvent::Exit(idx, status)) => {
                    self.command_exit(idx, status)?;
                }
                None => return Ok(futures::Async::Ready(())),
            }
        }
    }
}

pub fn update(
    w: futures::sync::mpsc::Sender<StateEvent>,
    idx: usize,
    event: &crate::eval::CommandEvent,
) -> impl futures::future::Future<Item = (), Error = Error> {
    match event {
        crate::eval::CommandEvent::CommandStart(cmd, args) => {
            w.send(crate::state::StateEvent::Start(
                idx,
                cmd.to_string(),
                args.to_vec(),
            ))
        }
        crate::eval::CommandEvent::Output(out) => {
            w.send(crate::state::StateEvent::Output(idx, out.to_vec()))
        }
        crate::eval::CommandEvent::CommandExit(status) => {
            w.send(crate::state::StateEvent::Exit(idx, *status))
        }
    }
    .map(|_| ())
    .map_err(|e| Error::Send { source: e })
}

#[derive(Debug, Clone)]
struct Command {
    cmd: String,
    args: Vec<String>,
    output: Vec<u8>,
    status: Option<std::process::ExitStatus>,
}

impl Command {
    fn new(cmd: &str, args: &[String]) -> Self {
        Self {
            cmd: cmd.to_string(),
            args: args.to_vec(),
            output: vec![],
            status: None,
        }
    }
}
