#[derive(Debug)]
pub enum Error {
    CommandRequired,
}

pub fn parse(line: &str) -> Result<(String, Vec<String>), Error> {
    // TODO
    let mut tokens = line.split_whitespace().map(|s| s.to_string());
    if let Some(cmd) = tokens.next() {
        Ok((cmd, tokens.collect()))
    } else {
        Err(Error::CommandRequired)
    }
}
