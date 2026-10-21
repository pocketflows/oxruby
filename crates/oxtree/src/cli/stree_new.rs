//! Port of the app's `lib/tasks/stree_new.rake` (`StreeNew`), the
//! replacement for `bin/rails stree_new:{check,write}[:modified]`:
//!
//! ```text
//! oxtree stree-new check|write [--modified] [--print-width=N]
//!                  [--source-files=GLOB]... [--oxfmt=PATH]
//! ```
//!
//! `check:modified` and `write:modified` are accepted for `--modified`, and
//! `FILES="a b"` targets specific files, as in the rake task. Files are
//! found through git, filtered by the source globs (`FNM_PATHNAME |
//! FNM_EXTGLOB`) and `.streerc`'s `--ignore-files` (plain `fnmatch`), then
//! formatted largest-first on a thread pool. `.erb` files go through the
//! app's `ErbEmbeddedFormatter` (`embedded.rs`), everything else through the
//! Ruby formatter with default options.
//!
//! Everything is relative to the repository root (`git rev-parse
//! --show-toplevel`, else the working directory), where the rake task runs:
//! `.streerc`, `.oxfmtrc.json`, `node_modules/.bin/oxfmt` and the printed
//! paths. The defaults are the app's `SYNTAX_TREE_CONFIG` (width 100) plus
//! `ErbEmbeddedFormatter::FILE_GLOBS`.
//!
//! Differences from the rake task:
//! - a missing `.streerc` means no ignores (the rake task raises);
//! - when an input needs Tailwind class sorting and oxfmt isn't installed,
//!   the classes are left unsorted and `[warn] <file>: oxfmt not found;
//!   Tailwind classes left unsorted` goes to stderr (user decision);
//! - files of equal size keep git's order (Ruby's `sort_by` is unstable).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use rayon::prelude::*;

use super::fnmatch::{FNM_EXTGLOB, FNM_PATHNAME, fnmatch};
use super::{Failure, Io, PARSE_ERROR_CLASS, Source, read_ruby, read_utf8, readlines_chomp};
use crate::oxfmt::{Backend, Oxfmt};

/// `SYNTAX_TREE_CONFIG[:source_files] + ErbEmbeddedFormatter::FILE_GLOBS`
pub const DEFAULT_SOURCE_FILES: &[&str] = &[
    "**/*.rb",
    "Rakefile",
    "**/*.rake",
    "Gemfile",
    "**/*.html.erb",
    "bin/generate-openapi-spec",
    "**/*.js.erb",
    "**/*.css.erb",
    "**/*.json.erb",
];
/// `SYNTAX_TREE_CONFIG[:print_width]`
pub const DEFAULT_PRINT_WIDTH: usize = 100;

const USAGE: &str = "usage: oxtree stree-new check|write [--modified] [--print-width=N] \
[--source-files=GLOB]... [--oxfmt=PATH]";

struct Config {
    action: &'static str,
    modified: bool,
    print_width: usize,
    source_files: Vec<String>,
    oxfmt: Option<PathBuf>,
    backend: Backend,
    /// Hidden: print the selected files, one per line, and stop.
    list: bool,
    /// Hidden: print `{path: formatted or "ERROR Class: message"}` as JSON
    /// for every selected file instead of checking (compare_with_stree).
    emit_json: bool,
}

/// `StreeNew::Result`
enum Status {
    Changed,
    Unformatted,
    Error,
}

struct Outcome {
    status: Status,
    message: String,
    warning: Option<String>,
}

pub fn run(args: &[String], cwd: &Path, io: &mut Io) -> i32 {
    let config = match parse(args) {
        Ok(c) => c,
        Err(message) => {
            let _ = writeln!(io.stderr, "oxtree stree-new: {message}\n{USAGE}");
            return 1;
        }
    };
    let root = git_toplevel(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let finder = Finder {
        root: &root,
        source_files: &config.source_files,
        ignores: None,
    };
    let mut finder = finder;

    let files = match std::env::var("FILES") {
        Ok(list) => {
            let requested: Vec<String> = list.split_whitespace().map(str::to_string).collect();
            finder.select_source_files(requested)
        }
        Err(_) if config.modified => finder.modified_files(),
        Err(_) => finder.all_files(),
    };

    if config.list {
        for file in &files {
            let _ = writeln!(io.stdout, "{file}");
        }
        return 0;
    }

    let oxfmt = Oxfmt::new(&root, config.oxfmt.clone(), config.backend);
    if config.emit_json {
        return emit_json(&files, &root, &config, &oxfmt, io);
    }
    run_action(&config, files, &root, &oxfmt, io)
}

fn parse(args: &[String]) -> Result<Config, String> {
    let mut config = Config {
        action: "",
        modified: false,
        print_width: DEFAULT_PRINT_WIDTH,
        source_files: Vec::new(),
        oxfmt: None,
        backend: Backend::Library,
        list: false,
        emit_json: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            match &inline {
                Some(v) => Ok(v.clone()),
                None => args
                    .next()
                    .cloned()
                    .ok_or(format!("missing argument: {what}")),
            }
        };
        match name {
            "check" | "write" | "check:modified" | "write:modified" if config.action.is_empty() => {
                config.action = if name.starts_with("check") {
                    "check"
                } else {
                    "write"
                };
                config.modified |= name.ends_with(":modified");
            }
            "--modified" => config.modified = true,
            "--print-width" => {
                let v = value(name)?;
                config.print_width = v
                    .parse()
                    .map_err(|_| format!("invalid argument: {name}={v}"))?;
            }
            "--source-files" => config.source_files.push(value(name)?),
            "--oxfmt" => config.oxfmt = Some(PathBuf::from(value(name)?)),
            "--oxfmt-backend" => {
                config.backend = match value(name)?.as_str() {
                    "library" => Backend::Library,
                    "process" => Backend::Process,
                    other => return Err(format!("invalid argument: {name}={other}")),
                }
            }
            "--list" => config.list = true,
            "--emit-json" => config.emit_json = true,
            _ => return Err(format!("invalid argument: {arg}")),
        }
    }
    if config.action.is_empty() && !config.list && !config.emit_json {
        return Err("expected check or write".to_string());
    }
    if config.action.is_empty() {
        config.action = "check";
    }
    if config.source_files.is_empty() {
        config.source_files = DEFAULT_SOURCE_FILES.iter().map(|s| s.to_string()).collect();
    }
    if config.oxfmt.is_none()
        && let Ok(bin) = std::env::var("OXFMT_BIN")
        && !bin.is_empty()
    {
        config.oxfmt = Some(PathBuf::from(bin));
    }
    Ok(config)
}

fn git_toplevel(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    Some(PathBuf::from(path.trim_end_matches(['\n', '\r'])))
}

struct Finder<'a> {
    root: &'a Path,
    source_files: &'a [String],
    /// `streerc_ignore_globs`, read on first use.
    ignores: Option<Vec<String>>,
}

impl Finder<'_> {
    /// `all_files`
    fn all_files(&mut self) -> Vec<String> {
        let mut files = self.git_lines(&["ls-files", "-z"]);
        files.extend(self.untracked_files());
        self.select_source_files(files)
    }

    /// `modified_files`: changed since the merge-base with main.
    fn modified_files(&mut self) -> Vec<String> {
        let merge_base = Command::new("git")
            .args(["merge-base", "main", "HEAD"])
            .current_dir(self.root)
            .stderr(Stdio::inherit())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let mut changed =
            self.git_lines(&["diff", "-z", "--name-only", "--diff-filter=d", &merge_base]);
        changed.extend(self.untracked_files());
        self.select_source_files(changed)
    }

    fn untracked_files(&self) -> Vec<String> {
        self.git_lines(&["ls-files", "-z", "--others", "--exclude-standard"])
    }

    /// `IO.popen(["git", *arguments], &:read).split("\0")`
    fn git_lines(&self, arguments: &[&str]) -> Vec<String> {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(self.root)
            .stderr(Stdio::inherit())
            .output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// `select_source_files`
    fn select_source_files(&mut self, files: Vec<String>) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for file in files {
            if !seen.insert(file.clone()) {
                continue;
            }
            if self.root.join(&file).is_file()
                && self.matches_source_glob(&file)
                && !self.ignored(&file)
            {
                out.push(file);
            }
        }
        out
    }

    fn matches_source_glob(&self, file: &str) -> bool {
        self.source_files
            .iter()
            .any(|p| fnmatch(p, file, FNM_PATHNAME | FNM_EXTGLOB))
    }

    /// `ignored?`: `.streerc`'s `--ignore-files=` globs, matched with plain
    /// `fnmatch` (so `*` crosses `/`) and without stripping quotes.
    fn ignored(&mut self, file: &str) -> bool {
        let root = self.root;
        let ignores = self.ignores.get_or_insert_with(|| {
            let text = std::fs::read(root.join(".streerc")).unwrap_or_default();
            readlines_chomp(&String::from_utf8_lossy(&text))
                .into_iter()
                .filter_map(|line| {
                    line.strip_prefix("--ignore-files=")
                        .filter(|g| !g.is_empty())
                        .map(str::to_string)
                })
                .collect()
        });
        ignores.iter().any(|glob| fnmatch(glob, file, 0))
    }
}

/// The formatted file, `Err` with the exception's class and message.
fn format_file(
    file: &str,
    root: &Path,
    print_width: usize,
    oxfmt: &Oxfmt,
) -> (Option<Source>, Result<String, (&'static str, String)>, bool) {
    let erb = super::extname(file) == ".erb";
    let bytes = match std::fs::read(root.join(file)) {
        Ok(b) => b,
        Err(e) => {
            return (
                None,
                Err(("SystemCallError", super::io_message(&e, file))),
                false,
            );
        }
    };
    let source = if erb {
        read_utf8(bytes)
    } else {
        read_ruby(bytes)
    };
    let source = match source {
        Ok(s) => s,
        Err(f) => return (None, Err(failure_class(f)), false),
    };
    if erb {
        crate::oxfmt::take_tailwind_unsorted();
        let result = crate::embedded::format(&source.text, print_width, file, oxfmt)
            .map_err(|e| (e.class_name(), e.message()));
        let unsorted = crate::oxfmt::take_tailwind_unsorted();
        (Some(source), result, unsorted)
    } else {
        let result = crate::format(&source.text, print_width, crate::format::Options::default())
            .map_err(|e| (PARSE_ERROR_CLASS, e.message));
        (Some(source), result, false)
    }
}

fn failure_class(f: Failure) -> (&'static str, String) {
    match f {
        Failure::Parse(e) => (PARSE_ERROR_CLASS, e.message),
        Failure::Other { class, message } => (class, message),
        Failure::Unformatted | Failure::NonIdempotent => ("StandardError", String::new()),
    }
}

/// `"#{e.class}: #{e.message.lines.first&.strip}"`
fn error_text(class: &str, message: &str) -> String {
    let first = message.split_inclusive('\n').next().unwrap_or("").trim();
    format!("{class}: {first}")
}

/// `process_file`
fn process_file(
    action: &str,
    file: &str,
    root: &Path,
    print_width: usize,
    oxfmt: &Oxfmt,
) -> Option<Outcome> {
    let (source, result, unsorted) = format_file(file, root, print_width, oxfmt);
    let warning =
        unsorted.then(|| format!("[warn] {file}: oxfmt not found; Tailwind classes left unsorted"));
    let outcome = match (source, result) {
        (_, Err((class, message))) => Outcome {
            status: Status::Error,
            message: format!("[error] {file}: {}", error_text(class, &message)),
            warning,
        },
        (Some(source), Ok(formatted)) => {
            if source.text == formatted {
                // Unchanged: no result, unless there is a warning to print
                // (which doesn't count as a failure).
                return warning.map(|w| Outcome {
                    status: Status::Changed,
                    message: String::new(),
                    warning: Some(w),
                });
            }
            if action == "write" {
                if let Err(e) = std::fs::write(root.join(file), source.encode(&formatted)) {
                    Outcome {
                        status: Status::Error,
                        message: format!(
                            "[error] {file}: {}",
                            error_text("SystemCallError", &super::io_message(&e, file))
                        ),
                        warning,
                    }
                } else {
                    Outcome {
                        status: Status::Changed,
                        message: file.to_string(),
                        warning,
                    }
                }
            } else {
                Outcome {
                    status: Status::Unformatted,
                    message: format!("[warn] {file}"),
                    warning,
                }
            }
        }
        (None, Ok(_)) => unreachable!(),
    };
    Some(outcome)
}

fn elapsed_ms(start: Instant) -> u128 {
    (start.elapsed().as_secs_f64() * 1000.0).round() as u128
}

fn nprocessors() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Largest first; equal sizes keep their order.
fn order(files: Vec<String>, root: &Path) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ordered: Vec<(u64, String)> = files
        .into_iter()
        .filter(|f| seen.insert(f.clone()))
        .map(|f| {
            (
                std::fs::metadata(root.join(&f))
                    .map(|m| m.len())
                    .unwrap_or(0),
                f,
            )
        })
        .collect();
    ordered.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    ordered.into_iter().map(|(_, f)| f).collect()
}

/// `StreeNew.run`
fn run_action(config: &Config, files: Vec<String>, root: &Path, oxfmt: &Oxfmt, io: &mut Io) -> i32 {
    let action = config.action;
    let start = Instant::now();
    if files.is_empty() {
        let _ = writeln!(io.stdout, "No files to {action} ({}ms)", elapsed_ms(start));
        return 0;
    }
    let ordered = order(files, root);
    let num_processes = nprocessors().min(ordered.len());
    let results: Vec<Outcome> = ordered
        .par_iter()
        .with_max_len(1)
        .filter_map(|file| process_file(action, file, root, config.print_width, oxfmt))
        .collect();

    for result in &results {
        if let Some(warning) = &result.warning {
            let _ = writeln!(io.stderr, "{warning}");
        }
        if !result.message.is_empty() {
            let _ = writeln!(io.stdout, "{}", result.message);
        }
    }
    let failed = results
        .iter()
        .filter(|r| !matches!(r.status, Status::Changed))
        .count();
    if failed > 0 {
        let _ = writeln!(
            io.stdout,
            "\n{action} completed with {failed} file(s) reporting errors ({}ms)",
            elapsed_ms(start)
        );
        1
    } else {
        let _ = writeln!(
            io.stdout,
            "\n{action} passed for {} files across {num_processes} processes ({}ms)",
            ordered.len(),
            elapsed_ms(start)
        );
        0
    }
}

/// Hidden `--emit-json`: every selected file's formatted output, or
/// `ERROR Class: message`, keyed by path. Never writes.
fn emit_json(files: &[String], root: &Path, config: &Config, oxfmt: &Oxfmt, io: &mut Io) -> i32 {
    let results: Vec<(String, String)> = files
        .par_iter()
        .with_max_len(1)
        .map(|file| {
            let (_, result, _) = format_file(file, root, config.print_width, oxfmt);
            let text = match result {
                Ok(formatted) => formatted,
                Err((class, message)) => format!("ERROR {}", error_text(class, &message)),
            };
            (file.clone(), text)
        })
        .collect();
    let map: serde_json::Map<String, serde_json::Value> = results
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect();
    let _ = writeln!(io.stdout, "{}", serde_json::Value::Object(map));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_text_takes_the_first_line() {
        assert_eq!(error_text("E", "  one  \ntwo"), "E: one");
        assert_eq!(error_text("E", ""), "E: ");
    }

    #[test]
    fn default_globs_select_like_the_rake_task() {
        let globs: Vec<String> = DEFAULT_SOURCE_FILES.iter().map(|s| s.to_string()).collect();
        let finder = Finder {
            root: Path::new("/"),
            source_files: &globs,
            ignores: None,
        };
        for (file, expected) in [
            ("app/models/user.rb", true),
            ("Rakefile", true),
            ("lib/tasks/a.rake", true),
            ("Gemfile", true),
            ("Gemfile.lock", false),
            ("app/views/a.html.erb", true),
            ("app/views/a.text.erb", false),
            ("app/views/a.js.erb", true),
            ("app/views/a.json.erb", true),
            ("bin/generate-openapi-spec", true),
            ("bin/rails", false),
            (".github/a.rb", false),
            ("config/.hidden.rb", false),
        ] {
            assert_eq!(finder.matches_source_glob(file), expected, "{file}");
        }
    }
}
