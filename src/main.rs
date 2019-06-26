#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
// clippy::cargo seems to be broken with rls currently
// #![warn(clippy::cargo)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::single_match)]
#![allow(clippy::write_with_newline)]

mod builtins;
mod eval;
mod parser;
mod process;
mod readline;
mod repl;

fn main() {
    repl::repl();
}
