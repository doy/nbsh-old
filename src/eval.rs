use snafu::futures01::StreamExt as _;
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

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, Error>;

pub fn eval(line: &str) -> Eval {
    Eval::new(line)
}

pub enum CommandEvent {
    CommandStart(String, Vec<String>),
    Output(Vec<u8>),
    CommandExit(std::process::ExitStatus),
}

pub struct Eval {
    line: String,
    stream: Option<
        Box<
            dyn futures::stream::Stream<Item = CommandEvent, Error = Error>
                + Send,
        >,
    >,
    manage_screen: bool,
}

impl Eval {
    pub fn new(line: &str) -> Self {
        Self {
            line: line.to_string(),
            stream: None,
            manage_screen: true,
        }
    }

    pub fn set_raw(mut self, raw: bool) -> Self {
        self.manage_screen = raw;
        self
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for Eval {
    type Item = CommandEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        if self.stream.is_none() {
            let line = self.line.as_ref();
            let (cmd, args) =
                crate::parser::parse(line).context(Parser { line })?;
            let builtin_stream = crate::builtins::Builtin::new(&cmd, &args);
            let stream: Box<
                dyn futures::stream::Stream<
                        Item = CommandEvent,
                        Error = Error,
                    > + Send,
            > = if let Ok(s) = builtin_stream {
                Box::new(s.context(BuiltinExecution { cmd }))
            } else {
                let process_stream =
                    crate::process::Process::new(&cmd, &args)
                        .context(Command { cmd: cmd.clone() })?
                        .set_raw(self.manage_screen);
                Box::new(process_stream.context(ProcessExecution { cmd }))
            };
            self.stream = Some(stream);
        }

        if let Some(ref mut stream) = &mut self.stream {
            stream.poll()
        } else {
            unreachable!()
        }
    }
}
