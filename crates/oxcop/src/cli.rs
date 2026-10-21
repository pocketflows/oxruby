//! Port of `lib/rubocop/cli.rb` and `cli/command/execute_runner.rb`.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};
use std::rc::Rc;

use clap::Parser;

use crate::cop::base::CopOptions;
use crate::cop::registry;
use crate::formatter::{self, FormatterOptions};
use crate::options::{AutocorrectMode, Options};
use crate::runner::Runner;
use crate::version::VERSION;

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_OFFENSES: i32 = 1;
pub const STATUS_ERROR: i32 = 2;

/// Where a run's output goes: the process's own streams, or (in `--server`
/// mode) a buffer that is sent back to the client.
#[derive(Clone)]
pub enum Sink {
    Stdout,
    Stderr,
    Buffer(Rc<RefCell<Vec<u8>>>),
}

impl Sink {
    pub fn buffer() -> Self {
        Sink::Buffer(Rc::default())
    }

    fn is_terminal(&self) -> bool {
        match self {
            Sink::Stdout => std::io::stdout().is_terminal(),
            Sink::Stderr => std::io::stderr().is_terminal(),
            Sink::Buffer(_) => false,
        }
    }

    pub fn take(&self) -> Vec<u8> {
        match self {
            Sink::Buffer(buffer) => std::mem::take(&mut buffer.borrow_mut()),
            _ => Vec::new(),
        }
    }
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Sink::Stdout => std::io::stdout().write(buf),
            Sink::Stderr => std::io::stderr().write(buf),
            Sink::Buffer(buffer) => buffer.borrow_mut().write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Sink::Stdout => std::io::stdout().flush(),
            Sink::Stderr => std::io::stderr().flush(),
            Sink::Buffer(_) => Ok(()),
        }
    }
}

/// The standard streams of one run.
pub struct Io {
    pub stdin: Box<dyn Read>,
    pub stdout: Sink,
    pub stderr: Sink,
}

impl Io {
    pub fn process() -> Self {
        Io {
            stdin: Box::new(std::io::stdin()),
            stdout: Sink::Stdout,
            stderr: Sink::Stderr,
        }
    }
}

thread_local! {
    static STDERR: RefCell<Sink> = const { RefCell::new(Sink::Stderr) };
}

/// The current run's stderr, for warnings printed deep inside a run.
pub fn stderr() -> Sink {
    STDERR.with(|sink| sink.borrow().clone())
}

thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `Kernel#warn` from inside a run: printed to stderr and kept for
/// `--spec-report` (the shim prints them to the spec's `$stderr`).
pub fn warn(message: &str) {
    if QUIET.with(std::cell::Cell::get) {
        return;
    }
    let _ = writeln!(stderr(), "{message}");
    WARNINGS.with(|w| w.borrow_mut().push(message.to_owned()));
}

/// The warnings `warn` printed so far in this run.
pub fn take_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

/// Runs `f` without warnings (replayed investigations, whose warnings were
/// already given).
pub fn quietly<R>(f: impl FnOnce() -> R) -> R {
    let previous = QUIET.with(|q| q.replace(true));
    let result = f();
    QUIET.with(|q| q.set(previous));
    result
}

pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    install_panic_hook();
    let args: Vec<OsString> = args.into_iter().collect();
    if args.get(1).is_some_and(|arg| arg == "--server") {
        return crate::server::serve();
    }
    run_with(args, &mut Io::process())
}

/// Panics inside cop callbacks are cop errors the Commissioner rescues and
/// reports, so they print nothing here. Other panics go to the run's
/// stderr (the `--server` client's buffer) or, outside the server, through
/// the default hook.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if crate::cop::base::in_cop_callback() {
            return;
        }
        match stderr() {
            Sink::Buffer(_) => {
                let _ = writeln!(stderr(), "thread 'main' {info}");
            }
            _ => default(info),
        }
    }));
}

/// Runs one command line with the given streams.
pub fn run_with(args: impl IntoIterator<Item = OsString>, io: &mut Io) -> i32 {
    let previous = STDERR.with(|sink| sink.replace(io.stderr.clone()));
    crate::config_loader::reset_warnings();
    take_warnings();
    let status = run_options(args, io);
    STDERR.with(|sink| sink.replace(previous));
    status
}

fn run_options(args: impl IntoIterator<Item = OsString>, io: &mut Io) -> i32 {
    let options = match Options::try_parse_from(args) {
        Ok(options) => options,
        Err(e) => {
            let status = if e.use_stderr() {
                STATUS_ERROR
            } else {
                STATUS_SUCCESS
            };
            let sink = if e.use_stderr() {
                &mut io.stderr
            } else {
                &mut io.stdout
            };
            if matches!(sink, Sink::Buffer(_)) {
                let _ = write!(sink, "{}", e.render());
            } else {
                let _ = e.print();
            }
            return status;
        }
    };
    crate::cop::mixin::util::set_inspect_ascii_only(
        options
            .default_external
            .as_deref()
            .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("UTF-8")),
    );

    if options.version {
        let _ = writeln!(io.stdout, "{VERSION}");
        return STATUS_SUCCESS;
    }
    if options.dump_ast || options.dump_tokens || options.dump_prism_tokens {
        return dump_ast(&options, io);
    }
    if options.show_ported_cops {
        for name in registry::names() {
            let _ = writeln!(io.stdout, "{name}");
        }
        return STATUS_SUCCESS;
    }
    if options.show_resolved_cops {
        return show_resolved_cops(&options, io);
    }

    // `OptionArgumentError`: the message alone.
    if let Err(message) = validate_compatibility(&options) {
        let _ = writeln!(io.stderr, "{message}");
        return STATUS_ERROR;
    }
    match execute_runner(&options, io) {
        Ok(status) => status,
        Err(message) => {
            let _ = writeln!(io.stderr, "Error: {message}");
            STATUS_ERROR
        }
    }
}

/// Hidden `--show-resolved-cops`: `config_store.for_pwd.for_cop(name)` for
/// every ported cop, with `Enabled` replaced by `Registry#enabled_cop_name?`
/// (whether the cop runs without `--only`).
fn show_resolved_cops(options: &Options, io: &mut Io) -> i32 {
    let mut store = crate::config_loader::ConfigStore::new(options.config.clone());
    store.pending_cops_option = if options.disable_pending_cops {
        Some(false)
    } else if options.enable_pending_cops {
        Some(true)
    } else {
        None
    };
    let config = match store.for_pwd() {
        Ok(config) => config,
        Err(e) => {
            let _ = writeln!(io.stderr, "Error: {e}");
            return STATUS_ERROR;
        }
    };
    let mut cops = serde_json::Map::new();
    for name in registry::names() {
        let mut params = config.for_cop(name).clone();
        params.insert(
            "Enabled".into(),
            serde_yaml_ng::Value::Bool(config.cop_enabled(name)),
        );
        cops.insert(name.to_owned(), yaml_to_json(&serde_yaml_ng::Value::Mapping(params)));
    }
    let _ = writeln!(io.stdout, "{}", serde_json::Value::Object(cops));
    STATUS_SUCCESS
}

/// JSON for `--show-resolved-cops`. JSON has no infinity (`Max: .inf`), so
/// non-finite floats become the strings `Infinity`, `-Infinity` and `NaN`,
/// as the compare script's Ruby side writes them.
fn yaml_to_json(value: &serde_yaml_ng::Value) -> serde_json::Value {
    use serde_json::Value as Json;
    use serde_yaml_ng::Value as Yaml;
    match value {
        Yaml::Null => Json::Null,
        Yaml::Bool(b) => Json::Bool(*b),
        Yaml::Number(n) => match n.as_f64() {
            Some(f) if f.is_nan() => Json::String("NaN".to_owned()),
            Some(f) if f.is_infinite() => {
                Json::String(if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned())
            }
            _ => serde_json::to_value(n).unwrap_or(Json::Null),
        },
        Yaml::String(s) => Json::String(s.clone()),
        Yaml::Sequence(items) => Json::Array(items.iter().map(yaml_to_json).collect()),
        Yaml::Mapping(mapping) => Json::Object(
            mapping
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        Yaml::String(s) => s.clone(),
                        other => serde_yaml_ng::to_string(other)
                            .unwrap_or_default()
                            .trim_end()
                            .to_owned(),
                    };
                    (key, yaml_to_json(v))
                })
                .collect(),
        ),
        Yaml::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}

/// The parts of `OptionsValidator#validate_compatibility` that apply.
fn validate_compatibility(options: &Options) -> Result<(), String> {
    // RuboCop's own specs (`--raw-config`) run the cop with `--only`.
    if !options.raw_config
        && options.only.iter().any(|cop| {
            cop == "Lint/RedundantCopDisableDirective" || cop == "RedundantCopDisableDirective"
        })
    {
        return Err("Lint/RedundantCopDisableDirective cannot be used with --only.".to_owned());
    }
    // `except_syntax?`
    if !options.raw_config
        && options
            .except
            .iter()
            .any(|cop| cop == "Lint/Syntax" || cop == "Syntax")
    {
        return Err("Syntax checking cannot be turned off.".to_owned());
    }
    Ok(())
}

fn execute_runner(options: &Options, io: &mut Io) -> Result<i32, String> {
    let format = options
        .format
        .clone()
        .unwrap_or_else(|| "progress".to_owned());
    let to_stderr = options.stderr;
    let color = if options.no_color {
        false
    } else if options.color {
        true
    } else if to_stderr {
        io.stderr.is_terminal()
    } else {
        io.stdout.is_terminal()
    };

    let cop_options = CopOptions {
        autocorrect: options.autocorrect_mode(),
        display_cop_names: options.display_cop_names(),
        json_format: matches!(format.as_str(), "json" | "j"),
        ignore_disable_comments: false,
        stdin: options.stdin.is_some(),
        lsp: options.lsp,
    };
    let mut runner = Runner::new(options, cop_options);
    // `report_pending_cops` (not for RuboCop's specs, which don't run the CLI).
    if !options.raw_config
        && !options.list_target_files
        && let Some(warning) = runner.pending_cops_warning().map_err(|e| e.to_string())?
    {
        let _ = write!(io.stderr, "{warning}");
    }

    if options.list_target_files {
        for file in runner.find_target_files().map_err(|e| e.to_string())? {
            let _ = writeln!(io.stdout, "{}", crate::path_util::smart_path(&file));
        }
        return Ok(STATUS_SUCCESS);
    }

    let output: Box<dyn Write> = if to_stderr {
        Box::new(io.stderr.clone())
    } else {
        Box::new(io.stdout.clone())
    };
    let formatter_options = FormatterOptions {
        color,
        safe_autocorrect: options.autocorrect_mode() == AutocorrectMode::Safe,
    };
    let mut formatter = formatter::build(&format, output, formatter_options)?;

    let run = runner.run(formatter.as_mut(), &mut io.stdin);
    drop(formatter);
    if let Some(path) = &options.spec_report {
        runner.spec_report.warnings = take_warnings();
        std::fs::write(path, runner.spec_report.to_json().to_string())
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let all_passed = run.map_err(|e| e.to_string())?;

    maybe_print_corrected_source(options, &format, runner.corrected_stdin.as_deref(), io);

    for error in &runner.errors {
        let _ = writeln!(io.stderr, "{error}");
    }
    Ok(if !runner.errors.is_empty() {
        STATUS_ERROR
    } else if all_passed {
        STATUS_SUCCESS
    } else {
        STATUS_OFFENSES
    })
}

fn maybe_print_corrected_source(
    options: &Options,
    format: &str,
    corrected: Option<&[u8]>,
    io: &mut Io,
) {
    if formatter::INTEGRATION_FORMATTERS.contains(&format) {
        return;
    }
    if options.stdin.is_none() || options.autocorrect_mode() == AutocorrectMode::Off {
        return;
    }
    let separator = "=".repeat(20);
    if options.stderr {
        let _ = writeln!(io.stderr, "{separator}");
    } else {
        let _ = writeln!(io.stdout, "{separator}");
    }
    let _ = io.stdout.write_all(corrected.unwrap_or_default());
    let _ = io.stdout.flush();
}

fn dump_ast(options: &Options, io: &mut Io) -> i32 {
    let out = &mut io.stdout;
    for path in &options.paths {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                let _ = writeln!(io.stderr, "{}: {e}", path.display());
                return STATUS_ERROR;
            }
        };
        let (source, _) = crate::source_encoding::decode_lossy(&bytes);
        let buffer = crate::processed_source::normalize_newlines(&source);
        let processed = crate::processed_source::ProcessedSource::new(
            &source,
            &buffer,
            options.dump_ast_version,
            path,
        );
        let _ = writeln!(out, "== {}", path.display());
        if processed.valid_syntax() && options.dump_prism_tokens {
            let (tokens, unknown) = crate::ast::token_builder::prism_tokens_checked(&processed);
            for token in tokens {
                let state = if token.ty == "BRACE_LEFT" {
                    token.state
                } else {
                    0
                };
                let _ = writeln!(out, "{} {}..{} {state:x}", token.ty, token.start, token.end);
            }
            for offset in unknown {
                let _ = writeln!(out, "?unknown {offset}");
            }
        } else if processed.valid_syntax() && options.dump_tokens {
            for token in processed.tokens() {
                let _ = writeln!(
                    out,
                    "{} {} {}..{}",
                    token.ty,
                    escape_text(token.raw.as_deref().unwrap_or(token.text.as_bytes())),
                    token.range.begin_pos,
                    token.range.end_pos
                );
            }
        } else if processed.valid_syntax() {
            let _ = write!(out, "{}", processed.tree().dump());
        } else {
            let _ = writeln!(out, "syntax error");
        }
    }
    STATUS_SUCCESS
}

fn escape_text(text: &[u8]) -> String {
    let mut out = String::from("\"");
    for &byte in text {
        if (0x20..=0x7e).contains(&byte) && byte != b'"' && byte != b'\\' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("\\x{byte:02X}"));
        }
    }
    out.push('"');
    out
}
