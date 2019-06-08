mod readline;
mod repl;

fn main() {
    let _screen = crossterm::RawScreen::into_raw_mode().unwrap();

    repl::repl();
}
