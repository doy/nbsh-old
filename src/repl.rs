use futures::future::{Future, IntoFuture};
use futures::stream::Stream;
use snafu::{ResultExt, Snafu};
use std::io::Write;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("error during read: {}", source))]
    ReadError { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    EvalError { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    PrintError { source: std::io::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn repl() {
    let loop_stream = futures::stream::unfold(false, |done| {
        if done {
            return None;
        }

        let repl = read().and_then(|line| {
            eprint!("running '{}'\r\n", line);
            eval(&line).fold(None, |acc, event| match event {
                crate::eval::CommandEvent::Output(out) => match print(&out) {
                    Ok(()) => futures::future::ok(acc),
                    Err(e) => futures::future::err(e),
                },
                crate::eval::CommandEvent::ProcessExit(status) => {
                    futures::future::ok(Some(format!("{}", status)))
                }
                crate::eval::CommandEvent::BuiltinExit => {
                    futures::future::ok(Some(format!("success")))
                }
            })
        });

        Some(repl.then(move |res| match res {
            Ok(Some(status)) => {
                eprint!("command exited: {}\r\n", status);
                return Ok((done, false));
            }
            Ok(None) => {
                eprint!("command exited weirdly?\r\n");
                return Ok((done, false));
            }
            Err(Error::ReadError {
                source: crate::readline::Error::EOF,
            }) => {
                return Ok((done, true));
            }
            Err(Error::EvalError {
                source:
                    crate::eval::Error::ParserError {
                        source: crate::parser::Error::CommandRequired,
                        line: _,
                    },
            }) => {
                return Ok((done, false));
            }
            Err(e) => {
                let stderr = std::io::stderr();
                let mut stderr = stderr.lock();
                // panics seem fine for errors during error handling
                write!(stderr, "{}\r\n", e).unwrap();
                stderr.flush().unwrap();
                return Ok((done, false));
            }
        }))
    });
    tokio::run(loop_stream.collect().map(|_| ()));
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline("$ ", true)
        .into_future()
        .flatten()
        .map_err(|e| Error::ReadError { source: e })
}

fn eval(
    line: &str,
) -> impl futures::stream::Stream<Item = crate::eval::CommandEvent, Error = Error>
{
    crate::eval::eval(line)
        .into_future()
        .flatten_stream()
        .map_err(|e| Error::EvalError { source: e })
}

fn print(out: &[u8]) -> Result<()> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write(out).context(PrintError)?;
    stdout.flush().context(PrintError)
}
