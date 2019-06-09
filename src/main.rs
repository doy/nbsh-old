mod builtins;
mod eval;
mod parser;
mod process;
mod readline;
mod repl;

fn main() {
    repl::repl();
}
