use futures::future::Future;
use std::io::Write;

#[derive(Debug)]
enum Error {
    ReadError(crate::readline::Error),
    // EvalError(std::io::Error),
    PrintError(std::io::Error),
    // LoopError,
}

pub fn repl() {
    tokio::run(tokio::prelude::future::lazy(|| {
        let mut done = false;
        while !done {
            let res = read()
                .and_then(move |line| eval(&line))
                .and_then(move |out| print(&out))
                .wait();
            match res {
                Ok(_) => {}
                Err(Error::ReadError(crate::readline::Error::EOF)) => {
                    done = true;
                }
                Err(e) => {
                    let stderr = std::io::stderr();
                    let mut stderr = stderr.lock();
                    write!(stderr, "error: {:?}", e).unwrap();
                    stderr.flush().unwrap();
                    done = true;
                }
            }
        }
        futures::future::ok(())
    }));
}

fn read() -> impl futures::future::Future<Item = String, Error = Error> {
    crate::readline::readline("$ ", true).map_err(|e| Error::ReadError(e))
}

fn eval(line: &str) -> Result<String, Error> {
    Ok(format!("got line '{}'\r\n", line))
}

fn print(out: &str) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout
        .write(out.as_bytes())
        .map_err(|e| Error::PrintError(e))?;
    stdout.flush().map_err(|e| Error::PrintError(e))
}
