use futures::future::{Future as _, IntoFuture as _};
use futures::sink::Sink as _;
use snafu::futures01::FutureExt as _;
use std::io::Write as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("error during read: {}", source))]
    Read { source: crate::readline::Error },

    #[snafu(display("error during eval: {}", source))]
    Eval { source: crate::eval::Error },

    #[snafu(display("error during print: {}", source))]
    Print { source: crate::state::Error },

    #[snafu(display("error during sending: {}", source))]
    Sending {
        source: futures::sync::mpsc::SendError<crate::state::StateEvent>,
    },

    #[snafu(display("error during receiving: {}", source))]
    Receiving {
        source: futures::sync::oneshot::Canceled,
    },
}

pub fn tui() {
    tokio::run(futures::lazy(|| {
        let (w, r) = futures::sync::mpsc::channel(0);

        tokio::spawn(crate::state::State::new(r).map_err(|e| {
            error(&Error::Print { source: e });
        }));

        futures::future::loop_fn(0, move |idx| {
            let w = w.clone();
            read()
                .and_then(move |line| {
                    let (res, req) = futures::sync::oneshot::channel();
                    w.send(crate::state::StateEvent::Line(idx, line, res))
                        .context(Sending)
                        .and_then(|_| req.context(Receiving))
                })
                .then(move |res| match res {
                    // successful run or empty input means prompt again
                    Ok(_)
                    | Err(Error::Eval {
                        source:
                            crate::eval::Error::Parser {
                                source: crate::parser::Error::CommandRequired,
                                ..
                            },
                    }) => Ok(futures::future::Loop::Continue(idx + 1)),
                    // eof means we're done
                    Err(Error::Read {
                        source: crate::readline::Error::EOF,
                    }) => Ok(futures::future::Loop::Break(())),
                    // any other errors should be displayed, then we
                    // prompt again
                    Err(e) => {
                        error(&e);
                        Ok(futures::future::Loop::Continue(idx + 1))
                    }
                })
        })
    }));
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline("$ ", true)
        .into_future()
        .flatten()
        .context(Read)
}

fn error(e: &Error) {
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    // panics seem fine for errors during error handling
    write!(stderr, "{}\r\n", e).unwrap();
    stderr.flush().unwrap();
}
