#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
// clippy::cargo seems to be broken with rls currently
// #![warn(clippy::cargo)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::if_not_else)]
// match_same_arms is buggy, doesn't notice differences due to match arm order
#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::similar_names)]
#![allow(clippy::single_match)]
#![allow(clippy::write_with_newline)]

mod async_stdin;
mod builtins;
mod eval;
mod key_reader;
mod parser;
mod readline;

pub mod repl;
pub mod tui;
