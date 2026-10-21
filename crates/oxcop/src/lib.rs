//! A Rust port of RuboCop's core. Module layout mirrors `lib/rubocop/` in
//! the upstream repository so each file has an obvious Ruby counterpart.

pub mod ast;
pub mod cli;
pub mod config;
pub mod config_loader;
pub mod cop;
pub mod formatter;
pub mod options;
pub mod path_util;
pub mod processed_source;
pub mod runner;
pub mod target_finder;
pub mod target_ruby;
pub mod version;
