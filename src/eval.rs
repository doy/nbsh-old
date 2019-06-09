use futures::stream::Stream;
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to parse command line '{}': {}", line, source))]
    ParserError {
        line: String,
        source: crate::parser::Error,
    },

    #[snafu(display("failed to find command `{}`: {}", cmd, source))]
    CommandError {
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

pub fn eval(line: &str) -> Result<Eval, Error> {
    Eval::new(line)
}

pub enum CommandEvent {
    Output(Vec<u8>),
    ProcessExit(std::process::ExitStatus),
    BuiltinExit,
}

pub struct Eval {
    stream: Box<
        dyn futures::stream::Stream<Item = CommandEvent, Error = Error>
            + Send,
    >,
}

impl Eval {
    fn new(line: &str) -> Result<Self, Error> {
        let (cmd, args) =
            crate::parser::parse(line).context(ParserError { line })?;
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
                Err(e) => return Err(e).context(CommandError { cmd }),
            }
        };
        Ok(Eval { stream })
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
