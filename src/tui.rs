use snafu::ResultExt as _;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("error from state: {}", source))]
    State { source: crate::state::Error },
}

pub fn tui() {
    let state = crate::state::State::new().context(State);
    match state {
        Ok(state) => tokio::run(state),
        Err(e) => eprintln!("failed to create state: {}", e),
    }
}
