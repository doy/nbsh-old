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

        let repl = read()
            .and_then(|line| {
                eval(&line).for_each(|event| {
                    futures::future::FutureResult::from(print(&event))
                })
            })
            .then(|res| match res {
                // successful run or empty input means prompt again
                Ok(_)
                | Err(Error::Eval {
                    source:
                        crate::eval::Error::Parser {
                            source: crate::parser::Error::CommandRequired,
                            ..
                        },
                }) => Ok((false, false)),
                // eof means we're done
                Err(Error::Read {
                    source: crate::readline::Error::EOF,
                }) => Ok((false, true)),
                // any other errors should be displayed, then we prompt again
                Err(e) => {
                    let stderr = std::io::stderr();
                    let mut stderr = stderr.lock();
                    // panics seem fine for errors during error handling
                    write!(stderr, "{}\r\n", e).unwrap();
                    stderr.flush().unwrap();
                    Ok((false, false))
                }
            });
        Some(repl)
    });
    let loop_future = loop_stream.collect().map(|_| ());
    tokio::run(loop_future);
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

fn print(event: &crate::eval::CommandEvent) -> Result<()> {
    match event {
        crate::eval::CommandEvent::CommandStart(cmd, args) => {
            eprint!("running '{} {:?}'\r\n", cmd, args);
        }
        crate::eval::CommandEvent::Output(out) => {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            stdout.write(out).context(Print)?;
            stdout.flush().context(Print)?;
        }
        crate::eval::CommandEvent::ProcessExit(status) => {
            eprint!("command exited: {}\r\n", status);
        }
        crate::eval::CommandEvent::BuiltinExit => {
            eprint!("builtin exited\r\n");
        }
    }
    Ok(())
}
