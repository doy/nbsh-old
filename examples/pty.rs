use futures::future::{Future, IntoFuture};
use tokio_pty_process::CommandExt;

fn main() {
    tokio::run(futures::future::lazy(move || {
        let master = tokio_pty_process::AsyncPtyMaster::open().unwrap();
        let args: Vec<&str> = vec![];
        let child = std::process::Command::new("false")
            .args(&args)
            .spawn_pty_async(&master)
            .unwrap();
        tokio::spawn(
            child
                .map(|status| {
                    eprintln!("got status {}", status);
                })
                .map_err(|_| ()),
        )
        .into_future()
        .wait()
        .unwrap();
        futures::future::ok(())
    }));
}
