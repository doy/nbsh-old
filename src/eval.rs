use futures::stream::Stream as _;
use snafu::ResultExt as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("failed to parse command line '{}': {}", line, source))]
    Parser {
        line: String,
        source: crate::parser::Error,
    },

    #[snafu(display("failed to find command `{}`: {}", cmd, source))]
    Command {
        cmd: String,
        source: crate::process::Error,
    },

    #[snafu(display("failed to run builtin command `{}`: {}", cmd, source))]
    BuiltinExecution {
        cmd: String,
        source: crate::builtins::Error,
    },

    #[snafu(display("failed to run executable `{}`: {}", cmd, source))]
    ProcessExecution {
        cmd: String,
        source: crate::process::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn eval(line: &str) -> Result<Eval> {
    Eval::new(line)
}

pub enum CommandEvent {
    CommandStart(String, Vec<String>),
    Output(Vec<u8>),
    CommandExit(std::process::ExitStatus),
}

pub struct Eval {
    stream: Box<
        dyn futures::stream::Stream<Item = CommandEvent, Error = Error>
            + Send,
    >,
}

impl Eval {
    fn new(line: &str) -> Result<Self> {
        let (cmd, args) =
            crate::parser::parse(line).context(Parser { line })?;
        let builtin_stream = crate::builtins::exec(&cmd, &args);
        let stream: Box<
            dyn futures::stream::Stream<Item = CommandEvent, Error = Error>
                + Send,
        > = if let Ok(s) = builtin_stream {
            Box::new(s.map_err(move |e| Error::BuiltinExecution {
                cmd: cmd.clone(),
                source: e,
            }))
        } else {
            let process_stream = crate::process::spawn(&cmd, &args);
            match process_stream {
                Ok(s) => {
                    Box::new(s.map_err(move |e| Error::ProcessExecution {
                        cmd: cmd.clone(),
                        source: e,
                    }))
                }
                Err(e) => {
                    return Err(e).context(Command { cmd });
                }
            }
        };
        Ok(Self { stream })
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for Eval {
    type Item = CommandEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}
