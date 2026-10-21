//! Port of `lib/rubocop/options.rb`. Only the options the runner currently
//! honours are declared; unknown options are a usage error (exit 2), as in
//! RuboCop.

use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AutocorrectMode {
    #[default]
    Off,
    /// `-a`: only cops whose correction is marked safe.
    Safe,
    /// `-A`: every correction, safe or not.
    All,
}

#[derive(Debug, Parser)]
#[command(
    name = "oxcop",
    about = "A Rust port of RuboCop",
    disable_version_flag = true,
    override_usage = "oxcop [options] [file1, file2, ...]"
)]
pub struct Options {
    /// Files or directories to inspect.
    pub paths: Vec<PathBuf>,

    /// Specify configuration file.
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Choose an output formatter (progress, simple, clang, json, quiet).
    #[arg(short = 'f', long = "format", value_name = "FORMATTER")]
    pub format: Option<String>,

    /// Run only the given cop(s), comma separated.
    #[arg(long = "only", value_name = "COP1,COP2", value_delimiter = ',')]
    pub only: Vec<String>,

    /// Exclude the given cop(s), comma separated.
    #[arg(long = "except", value_name = "COP1,COP2", value_delimiter = ',')]
    pub except: Vec<String>,

    /// Autocorrect offenses (only when it's safe).
    #[arg(short = 'a', long = "autocorrect")]
    pub autocorrect: bool,

    /// Autocorrect offenses (safe and unsafe).
    #[arg(short = 'A', long = "autocorrect-all")]
    pub autocorrect_all: bool,

    /// Pipe source from STDIN, using FILE in offense reports.
    #[arg(short = 's', long = "stdin", value_name = "FILE")]
    pub stdin: Option<PathBuf>,

    /// Write all output to stderr except for the autocorrected source.
    #[arg(long = "stderr")]
    pub stderr: bool,

    /// Display cop names in offense messages (default).
    #[arg(
        short = 'D',
        long = "display-cop-names",
        overrides_with = "no_display_cop_names"
    )]
    pub display_cop_names: bool,

    #[arg(long = "no-display-cop-names", overrides_with = "display_cop_names")]
    pub no_display_cop_names: bool,

    /// Force excluding files specified in the configuration `Exclude` even if
    /// they are explicitly passed as arguments.
    #[arg(long = "force-exclusion")]
    pub force_exclusion: bool,

    /// List all files RuboCop would inspect, then exit.
    #[arg(short = 'L', long = "list-target-files")]
    pub list_target_files: bool,

    /// Display version.
    #[arg(short = 'v', long = "version")]
    pub version: bool,

    /// Accepted for compatibility; result caching is not implemented yet.
    #[arg(short = 'C', long = "cache", value_name = "FLAG", num_args = 0..=1, default_missing_value = "true")]
    pub cache: Option<String>,

    /// Inspect files in parallel (the default).
    #[arg(short = 'P', long = "parallel", action = ArgAction::SetTrue)]
    pub parallel: bool,

    #[arg(long = "no-parallel", action = ArgAction::SetTrue)]
    pub no_parallel: bool,

    #[arg(long = "color", overrides_with = "no_color")]
    pub color: bool,

    #[arg(long = "no-color", overrides_with = "color")]
    pub no_color: bool,

    /// Print the translated AST of the given files (see `script/dump_ast`).
    #[arg(long = "dump-ast", hide = true)]
    pub dump_ast: bool,

    /// Print the `parser`-gem tokens of the given files.
    #[arg(long = "dump-tokens", hide = true)]
    pub dump_tokens: bool,

    /// Print the rebuilt `Prism.lex` stream of the given files (see
    /// `script/dump_prism_tokens`).
    #[arg(long = "dump-prism-tokens", hide = true)]
    pub dump_prism_tokens: bool,

    /// The Ruby version `--dump-ast` parses as.
    #[arg(long = "dump-ast-version", hide = true, default_value_t = 3.3)]
    pub dump_ast_version: f64,

    /// Use the `-c` file as the whole configuration, without RuboCop's
    /// defaults (what RuboCop's specs do; used by the spec shim).
    #[arg(long = "raw-config", hide = true)]
    pub raw_config: bool,

    /// Keep the same cop instances across autocorrect iterations, as the
    /// `expect_correction` loop of RuboCop's specs does.
    #[arg(long = "persist-cops", hide = true)]
    pub persist_cops: bool,

    /// Stop after one autocorrect pass (`expect_correction(loop: false)`).
    #[arg(long = "autocorrect-once", hide = true)]
    pub autocorrect_once: bool,

    /// Write what the spec shim needs besides the offenses (cop errors,
    /// `config_to_allow_offenses`, exclude limits) to FILE as JSON.
    #[arg(long = "spec-report", hide = true, value_name = "FILE")]
    pub spec_report: Option<PathBuf>,

    /// Before the `--stdin` source, investigate FILE as if it were NAME with
    /// the same cop instances (`NAME=FILE`, repeatable): a spec example
    /// keeps one cop object across its investigations.
    #[arg(long = "prior", hide = true, value_name = "NAME=FILE")]
    pub prior: Vec<String>,

    /// Report the `--stdin` path as given instead of expanding it (a spec's
    /// `processed_source.file_path` is the buffer name).
    #[arg(long = "keep-stdin-path", hide = true)]
    pub keep_stdin_path: bool,

    /// The `--auto-gen-config` state to start from (JSON, as `--spec-report`
    /// writes it).
    #[arg(long = "auto-gen-state", hide = true, value_name = "FILE")]
    pub auto_gen_state: Option<PathBuf>,

    /// Ruby's `Encoding.default_internal || Encoding.default_external` in the
    /// spec process, which `String#inspect` escapes for (`US-ASCII`: every
    /// non-ASCII character becomes `\uXXXX`). UTF-8 when not given.
    #[arg(long = "default-external", hide = true, value_name = "ENCODING")]
    pub default_external: Option<String>,

    /// `Lint/RedundantCopDisableDirective`'s offenses to check (JSON:
    /// `[{"cop_name", "line", "last_line"}]`), as its spec constructs it.
    #[arg(long = "offenses-to-check", hide = true, value_name = "FILE")]
    pub offenses_to_check: Option<PathBuf>,

    /// No warnings from the first investigation (the spec shim's autocorrect
    /// run repeats one it already reported).
    #[arg(long = "quiet-first-pass", hide = true)]
    pub quiet_first_pass: bool,

    /// `RuboCop::LSP.enabled?` for cops that report differently under LSP.
    #[arg(long = "lsp", hide = true)]
    pub lsp: bool,

    /// Print the names of cops that have been ported to oxcop, then exit.
    #[arg(long = "show-ported-cops", hide = true)]
    pub show_ported_cops: bool,
}

impl Options {
    pub fn autocorrect_mode(&self) -> AutocorrectMode {
        if self.autocorrect_all {
            AutocorrectMode::All
        } else if self.autocorrect {
            AutocorrectMode::Safe
        } else {
            AutocorrectMode::Off
        }
    }

    /// `None` means "use `AllCops: DisplayCopNames` from the config".
    pub fn display_cop_names(&self) -> Option<bool> {
        if self.no_display_cop_names {
            Some(false)
        } else if self.display_cop_names {
            Some(true)
        } else {
            None
        }
    }
}
