use futures::future::{Future as _, IntoFuture as _};
use futures::stream::Stream as _;
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: crate::state::Error },
}

pub fn repl() {
    tokio::run(futures::lazy(|| {
        let (w, r) = futures::sync::mpsc::channel(0);

        let state_stream = crate::state::State::new(r).map_err(|e| {
            error(&Error::Print { source: e });
        });
        tokio::spawn(state_stream.collect().map(|_| ()));

        let loop_stream =
            futures::stream::unfold((false, 0), move |(done, idx)| {
                if done {
                    return None;
                }
                let w = w.clone();

                let repl = read()
                    .and_then(move |line| {
                        eval(&line).for_each(move |event| {
                            let w = w.clone();
                            print(w, idx, &event)
                        })
                    })
                    .then(move |res| match res {
                        // successful run or empty input means prompt again
                        Ok(_)
                        | Err(Error::Eval {
                            source:
                                crate::eval::Error::Parser {
                                    source:
                                        crate::parser::Error::CommandRequired,
                                    ..
                                },
                        }) => Ok(((false, idx + 1), (false, idx + 1))),
                        // eof means we're done
                        Err(Error::Read {
                            source: crate::readline::Error::EOF,
                        }) => Ok(((false, idx + 1), (true, idx + 1))),
                        // any other errors should be displayed, then we
                        // prompt again
                        Err(e) => {
                            error(&e);
                            Ok(((false, idx + 1), (false, idx + 1)))
                        }
                    });
                Some(repl)
            });
        loop_stream.collect().map(|_| ())
    }));
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

fn print(
    w: futures::sync::mpsc::Sender<crate::state::StateEvent>,
    idx: usize,
    event: &crate::eval::CommandEvent,
) -> impl futures::future::Future<Item = (), Error = Error> {
    crate::state::update(w, idx, event)
        .map_err(|e| Error::Print { source: e })
}

fn error(e: &Error) {
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    // panics seem fine for errors during error handling
    write!(stderr, "{}\r\n", e).unwrap();
    stderr.flush().unwrap();
}
