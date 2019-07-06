use snafu::{OptionExt as _, ResultExt as _};

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("unknown builtin {}", cmd))]
    UnknownBuiltin { cmd: String },

    #[snafu(display(
        "not enough parameters for {} (got {}, expected {})",
        cmd, args.len(), expected
    ))]
    NotEnoughParams {
        cmd: String,
        args: Vec<String>,
        expected: u32,
    },

    #[snafu(display(
        "too many parameters for {} (got {}, expected {})",
        cmd, args.len(), expected
    ))]
    TooManyParams {
        cmd: String,
        args: Vec<String>,
        expected: u32,
    },

    #[snafu(display("failed to cd to {}: {}", dir, source))]
    Chdir { dir: String, source: nix::Error },

    #[snafu(display("failed to cd: $HOME not set"))]
    ChdirUnknownHome,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn exec(cmd: &str, args: &[String]) -> Result<Builtin> {
    Builtin::new(cmd, args)
}

pub struct Builtin {
    cmd: String,
    args: Vec<String>,
    started: bool,
    done: bool,
}

impl Builtin {
    fn new(cmd: &str, args: &[String]) -> Result<Self> {
        match cmd {
            "cd" => Ok(Self {
                cmd: cmd.to_string(),
                args: args.to_vec(),
                started: false,
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
        if !self.started {
            self.started = true;
            Ok(futures::Async::Ready(Some(
                crate::eval::CommandEvent::CommandStart(
                    self.cmd.clone(),
                    self.args.clone(),
                ),
            )))
        } else if !self.done {
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
        } else {
            Ok(futures::Async::Ready(None))
        }
    }
}

fn cd(args: &[String]) -> Result<()> {
    snafu::ensure!(
        args.len() <= 1,
        TooManyParams {
            cmd: "cd",
            args,
            expected: 1_u32,
        }
    );
    let dir = if let Some(dir) = args.get(0) {
        std::convert::From::from(dir)
    } else {
        std::env::var_os("HOME").context(ChdirUnknownHome)?
    };
    nix::unistd::chdir(dir.as_os_str()).context(Chdir {
        dir: dir.to_string_lossy(),
    })
}
