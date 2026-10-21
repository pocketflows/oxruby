#![forbid(unsafe_code)]
//! A Rust port of RuboCop's core. Module layout mirrors `lib/rubocop/` in
//! the upstream repository so each file has an obvious Ruby counterpart.

pub mod ast;
pub mod cli;
pub mod comment_config;
pub mod config;
pub mod config_loader;
pub mod cop;
pub mod directive_comment;
pub mod ext;
pub mod file_finder;
pub mod formatter;
pub mod gem_version;
pub mod magic_comment;
pub mod name_similarity;
pub mod options;
pub mod path_util;
pub mod prism;
pub mod processed_source;
pub mod project_index;
pub mod project_index_loader;
pub mod regexp_parser;
pub mod runner;
pub mod server;
pub mod source_encoding;
pub mod target_finder;
pub mod target_ruby;
pub mod version;
