#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
// clippy::cargo seems to be broken with rls currently
// #![warn(clippy::cargo)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::if_not_else)]
// match_same_arms is buggy, doesn't notice differences due to match arm order
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::single_match)]
#![allow(clippy::write_with_newline)]

mod builtins;
mod eval;
mod parser;
mod process;
mod readline;
mod state;

pub mod repl;
pub mod tui;