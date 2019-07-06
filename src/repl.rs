use futures::future::{Future as _, IntoFuture as _};
use futures::stream::Stream as _;
use snafu::ResultExt as _;
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: std::io::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn repl() {
    let loop_stream = futures::stream::unfold(false, |done| {
        if done {
            return None;
        }

        let repl = read().and_then(|line| {
            eval(&line).fold(None, |acc, event| match event {
                crate::eval::CommandEvent::CommandStart(cmd, args) => {
                    eprint!("running '{} {:?}'\r\n", cmd, args);
                    futures::future::ok(acc)
                }
                crate::eval::CommandEvent::Output(out) => match print(&out) {
                    Ok(()) => futures::future::ok(acc),
                    Err(e) => futures::future::err(e),
                },
                crate::eval::CommandEvent::ProcessExit(status) => {
                    futures::future::ok(Some(format!("{}", status)))
                }
                crate::eval::CommandEvent::BuiltinExit => {
                    futures::future::ok(Some("success".to_string()))
                }
            })
        });

        Some(repl.then(move |res| match res {
            Ok(Some(status)) => {
                eprint!("command exited: {}\r\n", status);
                Ok((done, false))
            }
            Ok(None) => {
                eprint!("command exited weirdly?\r\n");
                Ok((done, false))
            }
            Err(Error::Read {
                source: crate::readline::Error::EOF,
            }) => Ok((done, true)),
            Err(Error::Eval {
                source:
                    crate::eval::Error::Parser {
                        source: crate::parser::Error::CommandRequired,
                        ..
                    },
            }) => Ok((done, false)),
            Err(e) => {
                let stderr = std::io::stderr();
                let mut stderr = stderr.lock();
                // panics seem fine for errors during error handling
                write!(stderr, "{}\r\n", e).unwrap();
                stderr.flush().unwrap();
                Ok((done, false))
            }
        }))
    });
    tokio::run(loop_stream.collect().map(|_| ()));
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline("$ ", true)
        .into_future()
        .flatten()
        .map_err(|e| Error::Read { source: e })
}

fn eval(
    line: &str,
) -> impl futures::stream::Stream<Item = crate::eval::CommandEvent, Error = Error>
{
    crate::eval::eval(line)
        .into_future()
        .flatten_stream()
        .map_err(|e| Error::Eval { source: e })
}

fn print(out: &[u8]) -> Result<()> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write(out).context(Print)?;
    stdout.flush().context(Print)
}
