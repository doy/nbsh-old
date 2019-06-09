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
            eprint!("running '{}'\r\n", line);
            eval(&line).fold(None, |acc, event| match event {
                crate::process::ProcessEvent::Output(out) => {
                    match print(&out) {
                        Ok(()) => futures::future::ok(acc),
                        Err(e) => futures::future::err(e),
                    }
                }
                crate::process::ProcessEvent::Exit(status) => {
                    futures::future::ok(Some(status))
                }
            })
        });

        Some(repl.then(move |res| match res {
            Ok(Some(status)) => {
                eprint!("process exited with status {}\r\n", status);
                return Ok((done, false));
            }
            Ok(None) => {
                eprint!("process exited weirdly?\r\n");
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
                return Ok((done, false));
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
) -> impl futures::stream::Stream<Item = crate::process::ProcessEvent, Error = Error>
{
    crate::process::spawn(line)
        .into_future()
        .flatten_stream()
        .map_err(|e| Error::EvalError(e))
}

fn print(out: &[u8]) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write(out).map_err(|e| Error::PrintError(e))?;
    stdout.flush().map_err(|e| Error::PrintError(e))
}
