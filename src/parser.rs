use snafu::{OptionExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("No command given"))]
    CommandRequired,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn parse(line: &str) -> Result<(String, Vec<String>)> {
    // TODO
    let mut tokens = line
        .split_whitespace()
        .map(std::string::ToString::to_string);
    let cmd = tokens.next().context(CommandRequired)?;
    Ok((cmd, tokens.collect()))
}
