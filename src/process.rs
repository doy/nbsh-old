use futures::future::Future;
use futures::try_ready;
use tokio::io::AsyncRead;
use tokio_pty_process::CommandExt;

#[derive(Debug)]
pub enum Error {
    IOError(std::io::Error),
}

pub fn spawn(
    line: &str,
) -> Result<
    (
        PtyStream,
        impl futures::future::Future<
            Item = std::process::ExitStatus,
            Error = Error,
        >,
    ),
    Error,
> {
    let master = tokio_pty_process::AsyncPtyMaster::open()
        .map_err(|e| Error::IOError(e))?;
    let mut argv: Vec<_> = line.split(' ').collect();
    let cmd = argv.remove(0);
    let child = std::process::Command::new(cmd)
        .args(&argv)
        .spawn_pty_async(&master)
        .map_err(|e| Error::IOError(e))?
        .map_err(|e| Error::IOError(e));
    let stream = PtyStream::new(master);
    Ok((stream, child))
}

pub struct PtyStream {
    master: tokio_pty_process::AsyncPtyMaster,
    buf: Vec<u8>,
}

impl PtyStream {
    fn new(master: tokio_pty_process::AsyncPtyMaster) -> Self {
        let buf = Vec::with_capacity(4096);
        PtyStream { master, buf }
    }
}

#[must_use = "streams do nothing unless polled"]
impl futures::stream::Stream for PtyStream {
    type Item = Vec<u8>;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        self.buf.clear();
        let n = try_ready!(self
            .master
            .read_buf(&mut self.buf)
            .map_err(|e| { Error::IOError(e) }));
        if n > 0 {
            let bytes = self.buf[..n].to_vec();
            Ok(futures::Async::Ready(Some(bytes)))
        } else {
            Ok(futures::Async::NotReady)
        }
    }
}
