#![forbid(unsafe_code)]
//! A Rust port of RuboCop's core. Module layout mirrors `lib/rubocop/` in
//! the upstream repository so each file has an obvious Ruby counterpart.
//!
//! It is also a library for building an application's own `oxcop` binary
//! with its own cops compiled in: see [`Oxcop`], [`plugin::Plugin`],
//! [`prelude`] and `docs/PLUGINS.md`.

use std::ffi::OsString;
use std::process::ExitCode;

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
pub mod lockfile;
pub mod magic_comment;
pub mod name_similarity;
pub mod options;
pub mod path_util;
pub mod plugin;
pub mod prelude;
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

/// The regexp engine behind [`regex!`], for the macro's expansion in other
/// crates.
#[doc(hidden)]
pub use onig;

/// The `oxcop` command line, with any compiled-in plugins:
///
/// ```no_run
/// fn main() -> std::process::ExitCode {
///     oxcop::Oxcop::new()
///         .plugin(oxcop::plugin::Plugin::new("my-app-cops"))
///         .run()
/// }
/// ```
#[derive(Default)]
pub struct Oxcop {
    plugins: Vec<plugin::Plugin>,
}

impl Oxcop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compiles `plugin`'s cops and configuration into this binary.
    pub fn plugin(mut self, plugin: plugin::Plugin) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Checks the plugins and installs them for this process, without
    /// running anything ([`Oxcop::run`] does this first). Fails when a
    /// plugin's cop name is taken or its default configuration isn't a YAML
    /// mapping, and when the registry was already used or plugins were
    /// already installed.
    pub fn install(self) -> Result<(), plugin::PluginError> {
        plugin::install(self.plugins)
    }

    /// Runs the command line of this process (`std::env::args_os`), as the
    /// stock `oxcop` binary does, and returns its exit status.
    pub fn run(self) -> ExitCode {
        ExitCode::from(self.run_with_args(std::env::args_os()))
    }

    /// Runs one command line (`args` includes the program name) and returns
    /// the exit status: 0 no offenses, 1 offenses, 2 an error (an invalid
    /// plugin included).
    pub fn run_with_args(self, args: impl IntoIterator<Item = OsString>) -> u8 {
        if let Err(error) = self.install() {
            eprintln!("Error: {error}");
            return cli::STATUS_ERROR as u8;
        }
        u8::try_from(cli::run(args)).unwrap_or(cli::STATUS_ERROR as u8)
    }
}
