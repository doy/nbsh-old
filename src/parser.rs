use snafu::{OptionExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("No command given"))]
    CommandRequired,
}

pub fn parse(line: &str) -> Result<(String, Vec<String>), Error> {
    // TODO
    let mut tokens = line.split_whitespace().map(|s| s.to_string());
    let cmd = tokens.next().context(CommandRequired)?;
    Ok((cmd, tokens.collect()))
}
