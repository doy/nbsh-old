use futures::future::Future;
use tokio::io::AsyncRead;
use tokio_pty_process::CommandExt;

#[derive(Debug)]
pub enum Error {
    IOError(std::io::Error),
}

pub fn spawn(line: &str) -> Result<RunningProcess, Error> {
    RunningProcess::new(line)
}

pub enum ProcessEvent {
    Output(Vec<u8>),
    Exit(std::process::ExitStatus),
}

pub struct RunningProcess {
    pty: tokio_pty_process::AsyncPtyMaster,
    process: tokio_pty_process::Child,
    buf: Vec<u8>,
    output_done: bool,
    exit_done: bool,
}

impl RunningProcess {
    fn new(line: &str) -> Result<Self, Error> {
        let pty = tokio_pty_process::AsyncPtyMaster::open()
            .map_err(|e| Error::IOError(e))?;

        let mut argv: Vec<_> = line.split(' ').collect();
        let cmd = argv.remove(0);
        let process = std::process::Command::new(cmd)
            .args(&argv)
            .spawn_pty_async(&pty)
            .map_err(|e| Error::IOError(e))?;

        Ok(RunningProcess {
            pty,
            process,
            buf: Vec::with_capacity(4096),
            output_done: false,
            exit_done: false,
        })
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for RunningProcess {
    type Item = ProcessEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        if !self.output_done {
            self.buf.clear();
            let output_poll = self
                .pty
                .read_buf(&mut self.buf)
                .map_err(|e| Error::IOError(e));
            match output_poll {
                Ok(futures::Async::Ready(n)) => {
                    let bytes = self.buf[..n].to_vec();
                    return Ok(futures::Async::Ready(Some(
                        ProcessEvent::Output(bytes),
                    )));
                }
                Ok(futures::Async::NotReady) => {
                    return Ok(futures::Async::NotReady);
                }
                Err(_) => {
                    self.output_done = true;
                }
            }
        }

        if !self.exit_done {
            let exit_poll =
                self.process.poll().map_err(|e| Error::IOError(e));
            match exit_poll {
                Ok(futures::Async::Ready(status)) => {
                    self.exit_done = true;
                    return Ok(futures::Async::Ready(Some(
                        ProcessEvent::Exit(status),
                    )));
                }
                Ok(futures::Async::NotReady) => {
                    return Ok(futures::Async::NotReady);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(futures::Async::Ready(None))
    }
}
