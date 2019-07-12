use futures::future::Future as _;
use futures::stream::Stream as _;
use snafu::futures01::{FutureExt as _, StreamExt as _};
use snafu::ResultExt as _;
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: std::io::Error },
}

type Result<T> = std::result::Result<T, Error>;

pub fn repl() {
    tokio::run(futures::future::loop_fn((), |_| {
        read()
            .and_then(|line| eval(&line).for_each(|event| print(&event)))
            .then(|res| match res {
                // successful run or empty input means prompt again
                Ok(_)
                | Err(Error::Eval {
                    source:
                        crate::eval::Error::Parser {
                            source: crate::parser::Error::CommandRequired,
                            ..
                        },
                }) => Ok(futures::future::Loop::Continue(())),
                // eof means we're done
                Err(Error::Read {
                    source: crate::readline::Error::EOF,
                }) => Ok(futures::future::Loop::Break(())),
                // any other errors should be displayed, then we
                // prompt again
                Err(e) => {
                    let stderr = std::io::stderr();
                    let mut stderr = stderr.lock();
                    // panics seem fine for errors during error handling
                    write!(stderr, "{}\r\n", e).unwrap();
                    stderr.flush().unwrap();
                    Ok(futures::future::Loop::Continue(()))
                }
            })
    }))
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline().context(Read)
}

fn eval(
    line: &str,
) -> impl futures::stream::Stream<Item = crate::eval::CommandEvent, Error = Error>
{
    crate::eval::eval(line).context(Eval)
}

fn print(event: &crate::eval::CommandEvent) -> Result<()> {
    match event {
        crate::eval::CommandEvent::CommandStart(_, _) => {}
        crate::eval::CommandEvent::Output(out) => {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            stdout.write(out).context(Print)?;
            stdout.flush().context(Print)?;
        }
        crate::eval::CommandEvent::CommandExit(_) => {}
    }
    Ok(())
}
