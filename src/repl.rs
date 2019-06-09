use futures::future::{Future, IntoFuture};
use futures::stream::Stream;
use std::io::Write;

#[derive(Debug)]
enum Error {
    ReadError(crate::readline::Error),
    EvalError(crate::process::Error),
    PrintError(std::io::Error),
}

pub fn repl() {
    let loop_stream = futures::stream::unfold(false, |done| {
        if done {
            return None;
        }

        let repl = read().and_then(|line| {
            eval(&line).and_then(|(out, status)| {
                out
                    // print the results as they come in
                    .and_then(|out| print(&out))
                    // wait for all output to be finished
                    .collect()
                    // ignore io errors since we just keep reading even after
                    // the process exits and the other end of the pty is
                    // closed
                    .or_else(|_| futures::future::ok(vec![]))
                    // once the output is all processed, then wait on the
                    // process to exit
                    .and_then(|_| status)
            })
        });

        Some(repl.then(move |res| match res {
            Ok(status) => {
                eprint!("process exited with status {}\r\n", status);
                return Ok((done, false));
            }
            Err(Error::ReadError(crate::readline::Error::EOF)) => {
                return Ok((done, true));
            }
            Err(e) => {
                let stderr = std::io::stderr();
                let mut stderr = stderr.lock();
                write!(stderr, "error: {:?}\r\n", e).unwrap();
                stderr.flush().unwrap();
                return Err(());
            }
        }))
    });
    tokio::run(loop_stream.collect().map(|_| ()));
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline("$ ", true).map_err(|e| Error::ReadError(e))
}

fn eval(
    line: &str,
) -> impl futures::future::Future<
    Item = (
        impl futures::stream::Stream<Item = Vec<u8>, Error = Error>,
        impl futures::future::Future<
            Item = std::process::ExitStatus,
            Error = Error,
        >,
    ),
    Error = Error,
> {
    match crate::process::spawn(line) {
        Ok((out, status)) => Ok((
            out.map_err(|e| Error::EvalError(e)),
            status.map_err(|e| Error::EvalError(e)),
        )),
        Err(e) => Err(e).map_err(|e| Error::EvalError(e)),
    }
    .into_future()
}

fn print(out: &[u8]) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write(out).map_err(|e| Error::PrintError(e))?;
    stdout.flush().map_err(|e| Error::PrintError(e))
}
