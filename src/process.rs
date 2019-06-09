use futures::future::Future;
use std::io::{Read, Write};
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
    // TODO: tokio::io::Stdin is broken
    // input: tokio::io::Stdin,
    input: tokio::reactor::PollEvented2<EventedStdin>,
    buf: Vec<u8>,
    output_done: bool,
    exit_done: bool,
    _screen: crossterm::RawScreen,
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

        // TODO: tokio::io::stdin is broken (it's blocking)
        // let input = tokio::io::stdin();
        let input = tokio::reactor::PollEvented2::new(EventedStdin);

        Ok(RunningProcess {
            pty,
            process,
            input,
            buf: Vec::with_capacity(4096),
            output_done: false,
            exit_done: false,
            _screen: crossterm::RawScreen::into_raw_mode().unwrap(),
        })
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for RunningProcess {
    type Item = ProcessEvent;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let ready = mio::Ready::readable();
        let input_poll = self.input.poll_read_ready(ready);
        match input_poll {
            Ok(futures::Async::Ready(_)) => {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                let mut buf = vec![0; 4096];
                // TODO: async
                match stdin.read(&mut buf) {
                    Ok(n) => {
                        if n > 0 {
                            let bytes = buf[..n].to_vec();

                            // TODO: async
                            let res = self.pty.write_all(&bytes);
                            if let Err(e) = res {
                                return Err(Error::IOError(e));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(Error::IOError(e));
                    }
                }
            }
            _ => {}
        }
        // TODO: this could lose pending bytes if there is stuff to read in
        // the buffer but we don't read it all in the previous read call,
        // since i think we won't get another notification until new bytes
        // actually arrive even if there are bytes in the buffer
        if let Err(e) = self.input.clear_read_ready(ready) {
            return Err(Error::IOError(e));
        }

        if !self.output_done {
            self.buf.clear();
            let output_poll = self
                .pty
                .read_buf(&mut self.buf)
                .map_err(|e| Error::IOError(e));
            match output_poll {
                Ok(futures::Async::Ready(n)) => {
                    let bytes = self.buf[..n].to_vec();
                    let bytes: Vec<_> = bytes
                        .iter()
                        // replace \n with \r\n
                        .fold(vec![], |mut acc, &c| {
                            if c == b'\n' {
                                acc.push(b'\r');
                                acc.push(b'\n');
                            } else {
                                acc.push(c);
                            }
                            acc
                        });
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

struct EventedStdin;

impl mio::Evented for EventedStdin {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> std::io::Result<()> {
        let fd = 0 as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> std::io::Result<()> {
        let fd = 0 as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> std::io::Result<()> {
        let fd = 0 as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.deregister(poll)
    }
}
