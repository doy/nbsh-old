use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("unknown builtin {}", cmd))]
    UnknownBuiltin { cmd: String },

    #[snafu(display("failed to cd to {}: {}", dir, source))]
    Chdir { dir: String, source: nix::Error },

    #[snafu(display("cd requires a directory to be given"))]
    ChdirParams,
}

pub fn exec(cmd: &str, args: &[String]) -> Result<Builtin, Error> {
    Builtin::new(cmd, args)
}

pub struct Builtin {
    cmd: String,
    args: Vec<String>,
    done: bool,
}

impl Builtin {
    fn new(cmd: &str, args: &[String]) -> Result<Self, Error> {
        match cmd {
            "cd" => Ok(Builtin {
                cmd: cmd.to_string(),
                args: args.to_vec(),
                done: false,
            }),
            _ => Err(Error::UnknownBuiltin {
                cmd: cmd.to_string(),
            }),
        }
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for Builtin {
    type Item = crate::eval::CommandEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        if self.done {
            return Ok(futures::Async::Ready(None));
        }

        self.done = true;
        let res = match self.cmd.as_ref() {
            "cd" => cd(&self.args),
            _ => Err(Error::UnknownBuiltin {
                cmd: self.cmd.clone(),
            }),
        };
        res.map(|_| {
            futures::Async::Ready(Some(
                crate::eval::CommandEvent::BuiltinExit,
            ))
        })
    }
}

fn cd(args: &[String]) -> Result<(), Error> {
    if let Some(dir) = args.get(0) {
        nix::unistd::chdir(dir.as_str()).context(Chdir { dir: dir.clone() })
    } else {
        Err(Error::ChdirParams)
    }
}
