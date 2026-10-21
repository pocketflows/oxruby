//! Port of syntax_tree 6.3.0's `lib/syntax_tree/cli.rb`: the `stree` command
//! line (`check`, `debug`, `format`, `write`, `help`, `version`), with its
//! options, `.streerc` handling, `--ignore-files`, messages and exit codes.
//!
//! Also the `stree-new` subcommand, a port of the app's
//! `lib/tasks/stree_new.rake` (see `stree_new.rs`).
//!
//! The actions that print syntax_tree's Ruby objects (`ast`, `doc`, `expr`,
//! `json`, `match`, `search`, `ctags`) and the language server (`lsp`) are
//! not ported; they exit 1 with a message.
//!
//! Differences from upstream, all cosmetic: work runs on a thread pool but
//! output is printed in input order (upstream's order depends on thread
//! timing); the source line in a parse error's highlight isn't syntax
//! colored (upstream colors it only on a TTY); uncaught Ruby exceptions
//! print `oxtree: <message> (<class>)` without a backtrace; the help text
//! names the `oxtree` command (no `stree` executable is shipped).

pub mod fnmatch;
pub mod glob;
pub mod stree_new;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use crate::parser::ParseError;

/// `SyntaxTree::VERSION`, which `stree version` prints.
pub const SYNTAX_TREE_VERSION: &str = "6.3.0";
/// `SyntaxTree::DEFAULT_PRINT_WIDTH`
pub const DEFAULT_PRINT_WIDTH: usize = 80;

/// Where the CLI reads and writes. The binary passes the process's streams;
/// tests pass buffers.
pub struct Io<'a> {
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}

/// `Color`: these escapes are printed whether or not the stream is a TTY.
pub mod color {
    pub fn bold(s: &str) -> String {
        format!("\x1b[1m{s}\x1b[0m")
    }
    pub fn gray(s: &str) -> String {
        format!("\x1b[38;5;102m{s}\x1b[0m")
    }
    pub fn red(s: &str) -> String {
        format!("\x1b[1;31m{s}\x1b[0m")
    }
    pub fn yellow(s: &str) -> String {
        format!("\x1b[33m{s}\x1b[0m")
    }
}

/// `puts`: appends a newline unless the string already ends with one.
pub fn puts(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
    if !bytes.ends_with(b"\n") {
        out.push(b'\n');
    }
}

/// An uncaught Ruby exception: printed as `oxtree: message (Class)`, exit 1.
#[derive(Debug)]
pub struct Fatal {
    pub class: &'static str,
    pub message: String,
}

impl Fatal {
    fn new(class: &'static str, message: impl Into<String>) -> Self {
        Fatal {
            class,
            message: message.into(),
        }
    }
}

/// Entry point for the `oxtree` binary: `argv` excludes the program
/// name. Returns the exit status.
pub fn run(argv: &[String], cwd: &Path, io: &mut Io) -> i32 {
    match argv.first().map(String::as_str) {
        Some("stree-new" | "stree_new") => stree_new::run(&argv[1..], cwd, io),
        Some("--version") => {
            let _ = writeln!(
                io.stdout,
                "oxtree {} (syntax_tree {SYNTAX_TREE_VERSION}, w_syntax_tree-erb 0.12.0)",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        _ => match run_stree(argv, cwd, io) {
            Ok(status) => status,
            Err(fatal) => {
                let _ = writeln!(io.stderr, "oxtree: {} ({})", fatal.message, fatal.class);
                1
            }
        },
    }
}

/// The plugins upstream ships, which `--plugins` requires as
/// `syntax_tree/<name>`. Their effect is global: loading one in `.streerc`
/// and another on the command line enables both.
#[derive(Debug, Default, Clone, Copy)]
pub struct Plugins {
    pub single_quotes: bool,
    pub trailing_comma: bool,
    pub disable_auto_ternary: bool,
    pub erb: bool,
}

impl Plugins {
    fn require(&mut self, name: &str) -> Result<(), Fatal> {
        match name {
            "plugin/single_quotes" => self.single_quotes = true,
            "plugin/trailing_comma" => self.trailing_comma = true,
            "plugin/disable_auto_ternary" => self.disable_auto_ternary = true,
            "erb" => self.erb = true,
            _ => {
                return Err(Fatal::new(
                    "LoadError",
                    format!("cannot load such file -- syntax_tree/{name}"),
                ));
            }
        }
        Ok(())
    }

    /// `Formatter::Options.new` with the plugins' constants defined.
    pub fn format_options(&self) -> crate::format::Options {
        let mut options = crate::format::Options::default();
        if self.single_quotes {
            options.quote = "'";
        }
        options.trailing_comma = self.trailing_comma;
        options.disable_auto_ternary = self.disable_auto_ternary;
        options
    }
}

/// `CLI::Options`
#[derive(Debug, Clone)]
pub struct Options {
    pub ignore_files: Vec<String>,
    pub plugins: Plugins,
    pub print_width: usize,
    pub scripts: Vec<String>,
    pub extension: String,
    pub target_ruby_version: Option<(u32, u32, u32)>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            ignore_files: Vec::new(),
            plugins: Plugins::default(),
            print_width: DEFAULT_PRINT_WIDTH,
            scripts: Vec::new(),
            extension: ".rb".to_string(),
            target_ruby_version: None,
        }
    }
}

const LONG_OPTIONS: &[&str] = &[
    "ignore-files",
    "plugins",
    "print-width",
    "extension",
    "target-ruby-version",
];

impl Options {
    /// `Formatter::Options.new(target_ruby_version:)`
    pub fn formatter_options(&self) -> crate::format::Options {
        let mut options = self.plugins.format_options();
        if let Some(version) = self.target_ruby_version {
            options.target_ruby_version = version;
        }
        options
    }

    /// `OptionParser#parse!`: consumes the options (anywhere in the list,
    /// abbreviated to any unique prefix, `--opt=value` or `--opt value`) and
    /// returns the remaining arguments.
    pub fn parse(&mut self, arguments: Vec<String>) -> Result<Vec<String>, Fatal> {
        let mut rest = Vec::new();
        let mut args = arguments.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                rest.extend(args.by_ref());
                break;
            }
            if let Some(long) = arg.strip_prefix("--") {
                let (name, inline) = match long.split_once('=') {
                    Some((n, v)) => (n, Some(v.to_string())),
                    None => (long, None),
                };
                let candidates: Vec<&str> = if LONG_OPTIONS.contains(&name) {
                    vec![name]
                } else {
                    LONG_OPTIONS
                        .iter()
                        .copied()
                        .filter(|o| o.starts_with(name))
                        .collect()
                };
                let option = match candidates.as_slice() {
                    [one] => *one,
                    [] => {
                        return Err(Fatal::new(
                            "OptionParser::InvalidOption",
                            format!("invalid option: {arg}"),
                        ));
                    }
                    _ => {
                        return Err(Fatal::new(
                            "OptionParser::AmbiguousOption",
                            format!("ambiguous option: {arg}"),
                        ));
                    }
                };
                let value = match inline {
                    Some(v) => v,
                    None => match args.next() {
                        Some(v) => v,
                        None => {
                            return Err(Fatal::new(
                                "OptionParser::MissingArgument",
                                format!("missing argument: {arg}"),
                            ));
                        }
                    },
                };
                self.apply(option, &value, &arg)?;
            } else if arg.len() > 1 && arg.starts_with('-') {
                let short = &arg[1..];
                if let Some(script) = short.strip_prefix('e') {
                    let script = if script.is_empty() {
                        match args.next() {
                            Some(v) => v,
                            None => {
                                return Err(Fatal::new(
                                    "OptionParser::MissingArgument",
                                    "missing argument: -e",
                                ));
                            }
                        }
                    } else {
                        script.to_string()
                    };
                    self.scripts.push(script);
                } else {
                    let flag: String = arg.chars().take(2).collect();
                    return Err(Fatal::new(
                        "OptionParser::InvalidOption",
                        format!("invalid option: {flag}"),
                    ));
                }
            } else {
                rest.push(arg);
            }
        }
        Ok(rest)
    }

    fn apply(&mut self, option: &str, value: &str, arg: &str) -> Result<(), Fatal> {
        match option {
            "ignore-files" => {
                let glob = value
                    .strip_prefix('\'')
                    .and_then(|v| v.strip_suffix('\''))
                    .filter(|_| value.len() >= 2)
                    .unwrap_or(value);
                self.ignore_files.push(glob.to_string());
            }
            "plugins" => {
                for plugin in value.split(',').filter(|p| !p.is_empty()) {
                    self.plugins.require(plugin)?;
                }
            }
            "print-width" => {
                let shown = if arg.contains('=') {
                    arg.to_string()
                } else {
                    format!("{arg} {value}")
                };
                let width = parse_ruby_integer(value).ok_or_else(|| {
                    Fatal::new(
                        "OptionParser::InvalidArgument",
                        format!("invalid argument: {shown}"),
                    )
                })?;
                self.print_width = width.max(0) as usize;
            }
            "extension" => {
                self.extension = format!(".{}", value.strip_prefix('.').unwrap_or(value));
            }
            "target-ruby-version" => {
                self.target_ruby_version = Some(semantic_version(value));
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

/// OptionParser's `Integer` conversion: an optional sign, then decimal,
/// `0x`/`0b`/`0o`/`0` (octal) prefixed digits, with `_` between digits.
fn parse_ruby_integer(value: &str) -> Option<i64> {
    let (negative, digits) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let lower = digits.to_ascii_lowercase();
    let (radix, body) = if let Some(b) = lower.strip_prefix("0x") {
        (16, b)
    } else if let Some(b) = lower.strip_prefix("0b") {
        (2, b)
    } else if let Some(b) = lower.strip_prefix("0o") {
        (8, b)
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, &lower[1..])
    } else {
        (10, lower.as_str())
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return None;
    }
    let cleaned: String = body.chars().filter(|&c| c != '_').collect();
    let n = i64::from_str_radix(&cleaned, radix).ok()?;
    Some(if negative { -n } else { n })
}

/// `Formatter::SemanticVersion` compares like `Gem::Version`: by its numeric
/// segments, missing ones being 0.
fn semantic_version(value: &str) -> (u32, u32, u32) {
    let mut parts = value
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().unwrap_or(u32::MAX));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// `CLI::HELP`
pub fn help() -> String {
    let b = color::bold;
    format!(
        "{}
  Print out the AST corresponding to the given files

{}
  Check that the given files are formatted as syntax tree would format them

{}
  Print out a ctags-compatible index of the given files

{}
  Check that the given files can be formatted idempotently

{}
  Print out the doc tree that would be used to format the given files

{}
  Print out a pattern-matching Ruby expression that would match the first
  expression of the given files

{}
  Print out the formatted version of the given files

{}
  Print out the JSON representation of the given files

{}
  Print out a pattern-matching Ruby expression that would match the given files

{}
  Display this help message

{}
  Run syntax tree in language server mode

{}
  Search for the given pattern in the given files

{}
  Output the current version of syntax tree

{}
  Read, format, and write back the source of the given files

--ignore-files=...
  A glob pattern to ignore files when processing. This can be specified
  multiple times to ignore multiple patterns.

--plugins=...
  A comma-separated list of plugins to load.

--print-width=...
  The maximum line width to use when formatting.

-e ...
  Parse an inline string.

--extension=...
  A file extension matching the content passed in via STDIN or -e.
  Defaults to '.rb'.

--config=...
  Path to a configuration file. Defaults to .streerc in the current
  working directory.
",
        b("oxtree ast [--plugins=...] [--print-width=NUMBER] [-e SCRIPT] FILE"),
        b("oxtree check [--plugins=...] [--print-width=NUMBER] [-e SCRIPT] FILE"),
        b("oxtree ctags [-e SCRIPT] FILE"),
        b("oxtree debug [--plugins=...] [--print-width=NUMBER] [-e SCRIPT] FILE"),
        b("oxtree doc [--plugins=...] [-e SCRIPT] FILE"),
        b("oxtree expr [-e SCRIPT] FILE"),
        b("oxtree format [--plugins=...] [--print-width=NUMBER] [-e SCRIPT] FILE"),
        b("oxtree json [--plugins=...] [-e SCRIPT] FILE"),
        b("oxtree match [--plugins=...] [-e SCRIPT] FILE"),
        b("oxtree help"),
        b("oxtree lsp [--plugins=...] [--print-width=NUMBER]"),
        b("oxtree search PATTERN [-e SCRIPT] FILE"),
        b("oxtree version"),
        b("oxtree write [--plugins=...] [--print-width=NUMBER] [-e SCRIPT] FILE"),
    )
}

/// `CLI::ConfigFile`: one argument per line, read from `.streerc` in the
/// working directory unless `--config` names another file.
fn config_arguments(filepath: Option<&str>, cwd: &Path) -> Result<Vec<String>, Fatal> {
    let path = match filepath {
        Some(p) => {
            let resolved = cwd.join(p);
            if !readable(&resolved) {
                return Err(Fatal::new(
                    "ArgumentError",
                    format!("Invalid configuration file: {p}"),
                ));
            }
            resolved
        }
        None => cwd.join(".streerc"),
    };
    if !readable(&path) {
        return Ok(Vec::new());
    }
    let contents = std::fs::read(&path).map_err(|e| Fatal::new("IOError", e.to_string()))?;
    Ok(readlines_chomp(&String::from_utf8_lossy(&contents)))
}

/// `File.readlines(path, chomp: true)`
pub fn readlines_chomp(contents: &str) -> Vec<String> {
    let mut lines: Vec<String> = contents
        .split_inclusive('\n')
        .map(|l| {
            let l = l.strip_suffix('\n').unwrap_or(l);
            l.strip_suffix('\r').unwrap_or(l).to_string()
        })
        .collect();
    if contents.is_empty() {
        lines.clear();
    }
    lines
}

/// `File.readable?`
pub fn readable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
        && (std::fs::File::open(path).is_ok() || std::fs::read_dir(path).is_ok())
}

/// `File.writable?`
fn writable(path: &Path) -> bool {
    std::fs::OpenOptions::new().append(true).open(path).is_ok()
}

/// `File.extname`
pub fn extname(path: &str) -> &str {
    let base = path.rsplit('/').next().unwrap_or(path);
    let trimmed = base.trim_start_matches('.');
    match trimmed.rfind('.') {
        Some(i) if i + 1 < trimmed.len() => &trimmed[i..],
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Check,
    Debug,
    Format,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Handler {
    Ruby,
    Erb,
}

/// `HANDLERS[extension]`: `.erb` and `.html.erb` are registered by the erb
/// plugin; everything else is Ruby.
fn handler_for(extension: &str, plugins: &Plugins) -> Handler {
    if plugins.erb && (extension == ".erb" || extension == ".html.erb") {
        Handler::Erb
    } else {
        Handler::Ruby
    }
}

/// A source as read: UTF-8 text, or a single-byte encoding (from a magic
/// comment) decoded byte-per-char so it round-trips.
#[derive(Debug, Clone)]
pub struct Source {
    pub text: String,
    pub latin1: bool,
}

impl Source {
    pub fn utf8(text: String) -> Self {
        Source {
            text,
            latin1: false,
        }
    }

    pub fn encode(&self, s: &str) -> Vec<u8> {
        if self.latin1 {
            s.chars()
                .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
                .collect()
        } else {
            s.as_bytes().to_vec()
        }
    }
}

/// Why an item failed.
#[derive(Debug)]
pub enum Failure {
    /// `Parser::ParseError`
    Parse(ParseError),
    /// `Check::UnformattedError`
    Unformatted,
    /// `Debug::NonIdempotentFormatError`
    NonIdempotent,
    /// Any other `StandardError`.
    Other {
        class: &'static str,
        message: String,
    },
}

/// The Ruby class syntax_tree's parse errors carry.
pub const PARSE_ERROR_CLASS: &str = "SyntaxTree::Parser::ParseError";

/// `SyntaxTree.read`: honors a magic encoding comment. Sources that aren't
/// valid UTF-8 are read byte-per-char when the comment names a single-byte
/// encoding, and are a parse error (as Ripper reports them) otherwise.
pub fn read_ruby(bytes: Vec<u8>) -> Result<Source, Failure> {
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Source::utf8(text)),
        Err(err) => {
            let bytes = err.into_bytes();
            if magic_encoding_is_single_byte(&bytes) {
                return Ok(Source {
                    text: bytes.iter().map(|&b| b as char).collect(),
                    latin1: true,
                });
            }
            Err(Failure::Parse(invalid_multibyte(&bytes)))
        }
    }
}

/// `File.read` (default external UTF-8), as the ERB handlers read.
pub fn read_utf8(bytes: Vec<u8>) -> Result<Source, Failure> {
    String::from_utf8(bytes)
        .map(Source::utf8)
        .map_err(|_| Failure::Other {
            class: "ArgumentError",
            message: "invalid byte sequence in UTF-8".to_string(),
        })
}

fn invalid_multibyte(bytes: &[u8]) -> ParseError {
    let valid = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    };
    let before = &bytes[..valid];
    let lineno = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    ParseError {
        message: "invalid multibyte char (UTF-8)".to_string(),
        lineno,
        column: valid - line_start,
    }
}

fn magic_encoding_is_single_byte(bytes: &[u8]) -> bool {
    let mut lines = bytes.split(|&b| b == b'\n');
    let mut header: Vec<&[u8]> = Vec::new();
    if let Some(first) = lines.next() {
        header.push(first);
        if first.starts_with(b"#!")
            && let Some(second) = lines.next()
        {
            header.push(second);
        }
    }
    for line in header {
        let line = String::from_utf8_lossy(line).to_ascii_lowercase();
        if !line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(i) = line.find("coding") {
            let rest = line[i + 6..].trim_start_matches([':', '=']).trim_start();
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                .collect();
            return name == "binary"
                || name == "ascii-8bit"
                || name == "us-ascii"
                || name.starts_with("iso-8859-")
                || name.starts_with("iso8859-")
                || name.starts_with("windows-125")
                || name.starts_with("cp125");
        }
    }
    false
}

/// What the stree CLI processes: a file, a `-e` script or stdin.
enum Item {
    File(String),
    Script(String),
    Stdin(String),
}

impl Item {
    fn filepath(&self) -> &str {
        match self {
            Item::File(p) => p,
            Item::Script(_) => "script",
            Item::Stdin(_) => "stdin",
        }
    }
}

/// The output of one item, printed after all items are done.
#[derive(Default)]
struct ItemOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    errored: bool,
}

fn run_stree(argv: &[String], cwd: &Path, io: &mut Io) -> Result<i32, Fatal> {
    let name = argv.first().cloned().unwrap_or_default();
    let mut arguments: Vec<String> = argv.iter().skip(1).cloned().collect();

    // First, check for a --config option so a custom config file applies.
    let mut config_filepath: Option<String> = None;
    for index in 0..arguments.len() {
        let arg = &arguments[index];
        if let Some(path) = arg.strip_prefix("--config=") {
            config_filepath = Some(path.to_string());
            arguments.remove(index);
            break;
        } else if arg == "--config" && index + 1 < arguments.len() {
            config_filepath = Some(arguments[index + 1].clone());
            arguments.drain(index..index + 2);
            break;
        }
    }
    let mut all = config_arguments(config_filepath.as_deref(), cwd)?;
    all.extend(arguments);

    let mut options = Options::default();
    let arguments = options.parse(all)?;

    let action = match name.as_str() {
        "c" | "check" => Action::Check,
        "debug" => Action::Debug,
        "f" | "format" => Action::Format,
        "w" | "write" => Action::Write,
        "help" => {
            let _ = io.stdout.write_all(help().as_bytes());
            return Ok(0);
        }
        "version" => {
            let _ = writeln!(io.stdout, "{SYNTAX_TREE_VERSION}");
            return Ok(0);
        }
        "a" | "ast" | "ctags" | "doc" | "e" | "expr" | "j" | "json" | "lsp" | "m" | "match"
        | "s" | "search" => {
            let _ = writeln!(io.stderr, "oxtree: `{name}` is not supported");
            return Ok(1);
        }
        _ => {
            let _ = io.stderr.write_all(help().as_bytes());
            return Ok(1);
        }
    };

    let mut queue = Vec::new();
    if !arguments.is_empty() || !options.scripts.is_empty() {
        for pattern in &arguments {
            for filepath in glob::glob(pattern, cwd) {
                if !readable(&cwd.join(&filepath)) {
                    continue;
                }
                if options
                    .ignore_files
                    .iter()
                    .any(|g| fnmatch::fnmatch(g, &filepath, 0))
                {
                    continue;
                }
                queue.push(Item::File(filepath));
            }
        }
        for script in &options.scripts {
            queue.push(Item::Script(script.clone()));
        }
    } else {
        let mut buf = Vec::new();
        let _ = io.stdin.read_to_end(&mut buf);
        queue.push(Item::Stdin(String::from_utf8_lossy(&buf).into_owned()));
    }

    let outputs: Vec<ItemOutput> = queue
        .par_iter()
        .with_max_len(1)
        .map(|item| run_item(action, item, &options, cwd))
        .collect();

    let mut errored = false;
    for output in outputs {
        let _ = io.stdout.write_all(&output.stdout);
        let _ = io.stderr.write_all(&output.stderr);
        errored |= output.errored;
    }
    if errored {
        match action {
            Action::Check => {
                let _ = writeln!(
                    io.stderr,
                    "The listed files did not match the expected format."
                );
            }
            Action::Debug => {
                let _ = writeln!(
                    io.stderr,
                    "The listed files could not be formatted idempotently."
                );
            }
            _ => {}
        }
        Ok(1)
    } else {
        match action {
            Action::Check => {
                let _ = writeln!(io.stdout, "All files matched expected format.");
            }
            Action::Debug => {
                let _ = writeln!(io.stdout, "All files can be formatted idempotently.");
            }
            _ => {}
        }
        Ok(0)
    }
}

fn format_with(handler: Handler, source: &str, options: &Options) -> Result<String, Failure> {
    match handler {
        Handler::Ruby => crate::format(source, options.print_width, options.formatter_options())
            .map_err(Failure::Parse),
        // `ERB.format` ignores `options:`; the Ruby inside tags gets a fresh
        // `Formatter::Options`, so only the plugins apply.
        Handler::Erb => crate::erb::format_with_options(
            source,
            options.print_width,
            options.plugins.format_options(),
        )
        .map_err(Failure::Parse),
    }
}

fn run_item(action: Action, item: &Item, options: &Options, cwd: &Path) -> ItemOutput {
    let mut out = ItemOutput::default();
    let start = Instant::now();
    let extension = match item {
        Item::File(path) => extname(path).to_string(),
        _ => options.extension.clone(),
    };
    let handler = handler_for(&extension, &options.plugins);
    let disk: PathBuf = match item {
        Item::File(path) => cwd.join(path),
        _ => PathBuf::new(),
    };

    let source = match item {
        Item::File(_) => match std::fs::read(&disk) {
            Ok(bytes) => match handler {
                Handler::Ruby => read_ruby(bytes),
                Handler::Erb => read_utf8(bytes),
            },
            Err(e) => Err(Failure::Other {
                class: "SystemCallError",
                message: io_message(&e, item.filepath()),
            }),
        },
        Item::Script(s) | Item::Stdin(s) => Ok(Source::utf8(s.clone())),
    };

    let result: Result<(), Failure> =
        source
            .as_ref()
            .map_err(clone_failure)
            .and_then(|source| match action {
                Action::Check => {
                    let formatted = format_with(handler, &source.text, options)?;
                    if formatted != source.text {
                        return Err(Failure::Unformatted);
                    }
                    Ok(())
                }
                Action::Debug => {
                    let formatted = format_with(handler, &source.text, options)?;
                    let double = format_with(handler, &formatted, options)?;
                    if formatted != double {
                        return Err(Failure::NonIdempotent);
                    }
                    Ok(())
                }
                Action::Format => {
                    let formatted = format_with(handler, &source.text, options)?;
                    puts(&mut out.stdout, &source.encode(&formatted));
                    Ok(())
                }
                Action::Write => {
                    let formatted = format_with(handler, &source.text, options)?;
                    let changed = formatted != source.text;
                    if changed
                        && matches!(item, Item::File(_))
                        && writable(&disk)
                        && let Err(e) = std::fs::write(&disk, source.encode(&formatted))
                    {
                        return Err(Failure::Other {
                            class: "SystemCallError",
                            message: io_message(&e, item.filepath()),
                        });
                    }
                    let filepath = item.filepath();
                    let shown = if changed {
                        filepath.to_string()
                    } else {
                        color::gray(filepath)
                    };
                    let delta = start.elapsed().as_secs_f64() * 1000.0;
                    puts(
                        &mut out.stdout,
                        format!("{shown} {}ms", delta.round() as i64).as_bytes(),
                    );
                    Ok(())
                }
            });

    let Err(failure) = result else {
        return out;
    };
    // The actions' own `rescue StandardError` blocks.
    match action {
        Action::Check | Action::Debug => {
            let line = format!("[{}] {}", color::yellow("warn"), item.filepath());
            puts(&mut out.stderr, line.as_bytes());
        }
        Action::Write => puts(&mut out.stdout, item.filepath().as_bytes()),
        Action::Format => {}
    }
    // `process_queue`'s rescue clauses.
    out.errored = true;
    match failure {
        Failure::Parse(error) => {
            puts(
                &mut out.stderr,
                format!("Error: {}", error.message).as_bytes(),
            );
            let text = source.as_ref().map(|s| s.text.as_str()).unwrap_or("");
            highlight_error(&mut out.stderr, &error, text);
        }
        Failure::Unformatted | Failure::NonIdempotent => {}
        Failure::Other { message, .. } => puts(&mut out.stderr, message.as_bytes()),
    }
    out
}

fn clone_failure(f: &Failure) -> Failure {
    match f {
        Failure::Parse(e) => Failure::Parse(e.clone()),
        Failure::Unformatted => Failure::Unformatted,
        Failure::NonIdempotent => Failure::NonIdempotent,
        Failure::Other { class, message } => Failure::Other {
            class,
            message: message.clone(),
        },
    }
}

/// A Ruby `Errno` message for a failed read or write.
pub fn io_message(e: &std::io::Error, path: &str) -> String {
    let text = match e.raw_os_error() {
        Some(21) => "Is a directory".to_string(),
        _ => match e.kind() {
            std::io::ErrorKind::NotFound => "No such file or directory".to_string(),
            std::io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
            _ => e.to_string(),
        },
    };
    format!("{text} @ rb_sysopen - {path}")
}

/// `CLI.highlight_error`: the lines around the error, with a caret under
/// the column.
fn highlight_error(err: &mut Vec<u8>, error: &ParseError, source: &str) {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let maximum = (error.lineno + 3).min(lines.len());
    if maximum == 0 {
        return;
    }
    // `Math.log10(maximum).ceil`: 1 for line 10, as upstream.
    let digits = (maximum as f64).log10().ceil() as usize;
    let first = error.lineno.saturating_sub(3);
    for (line_index, line) in lines.iter().enumerate().take(maximum).skip(first) {
        let line_number = line_index + 1;
        if line_number == error.lineno {
            let part2 = color::gray(&format!("{line_number:>digits$} |"));
            puts(
                err,
                format!("{} {part2} {line}", color::red(">")).as_bytes(),
            );
            let part3 = color::gray(&format!("  {:>digits$} |", " "));
            puts(
                err,
                format!("{part3} {}{}", " ".repeat(error.column), color::red("^")).as_bytes(),
            );
        } else {
            let prefix = color::gray(&format!("  {line_number:>digits$} |"));
            puts(err, format!("{prefix} {line}").as_bytes());
        }
    }
}

#[cfg(test)]
mod tests;
