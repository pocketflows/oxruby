//! The flag-style command line, for formatting a project the way rufo,
//! prettier and oxfmt do:
//!
//! ```text
//! oxtree [-w | --write | -c | --check | -l | --list-different] [options] [PATH...]
//! ```
//!
//! `--write` (the default) formats files in place, `--check` reports the
//! files that aren't formatted and `--list-different` prints only their
//! paths. A PATH is a file, a directory (its Ruby files and its
//! `*.html.erb`, `*.js.erb`, `*.css.erb` and `*.json.erb` files, leaving out
//! dot-directories and what git ignores) or a quoted `Dir.glob` pattern.
//! With no PATH, stdin is formatted to stdout.
//!
//! ERB files are formatted the way the app's `stree_new` task does, with
//! its `ErbEmbeddedFormatter` (`embedded.rs`): the ERB plugin's formatting,
//! with `<script>`/`<style>` bodies and whole `.js.erb`/`.css.erb`/
//! `.json.erb` files formatted by oxfmt. oxfmt's `.oxfmtrc.json` and its
//! executable (`node_modules/.bin/oxfmt`, or `OXFMT_BIN`) are looked up in
//! the git repository's root, else the working directory. When Tailwind
//! classes need sorting and the executable isn't there, they are left
//! unsorted with a warning. `--plugins=erb` is accepted and changes nothing.
//!
//! Everything else is `stree`'s: the options, `.streerc` (or `--config`),
//! `--ignore-files`, and the per-file output of `stree write` and `stree
//! check`. Differences:
//! - the print width defaults to 100;
//! - files are formatted largest first (output stays in input order);
//! - an `--ignore-files` glob that matches a directory skips everything
//!   under it;
//! - a PATH that is neither a file, a directory nor a glob is an error;
//! - the exit status is 0 when everything is formatted, 1 when files aren't
//!   (`--check`, `--list-different`) and 2 on errors (parse errors,
//!   unreadable files, bad options), as with rubocop.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rayon::prelude::*;

use super::fnmatch::fnmatch;
use super::{
    Action, Failure, Fatal, Handler, Io, Item, ItemOutput, Options, PARSE_ERROR_CLASS,
    config_arguments, extname, format_with, glob, handler_for, io_message, read_ruby, read_utf8,
    run_item,
};
use crate::oxfmt::{Backend, Oxfmt};

/// The print width when neither `.streerc` nor the command line sets one.
pub const DEFAULT_PRINT_WIDTH: usize = 100;

/// `stree`'s actions, which keep `stree`'s command line.
const STREE_ACTIONS: &[&str] = &[
    "a", "ast", "c", "check", "ctags", "debug", "doc", "e", "expr", "f", "format", "help", "j",
    "json", "lsp", "m", "match", "s", "search", "version", "w", "write",
];

/// Ruby files a directory PATH expands to, besides `*.rb`.
const RUBY_EXTENSIONS: &[&str] = &[".rb", ".rake", ".gemspec", ".ru"];
const RUBY_FILENAMES: &[&str] = &["Gemfile", "Rakefile"];
/// ERB files a directory PATH expands to: the app's
/// `ErbEmbeddedFormatter::FILE_GLOBS` (`**/*.html.erb`, ...), so not
/// `.text.erb` and the like.
const ERB_SUFFIXES: &[&str] = &[".html.erb", ".js.erb", ".css.erb", ".json.erb"];

const HELP: &str = "\
Usage: oxtree [options] [PATH...]

Formats Ruby files in place, or checks that they are formatted. A PATH is a
file, a directory (its Ruby files and its .html.erb, .js.erb, .css.erb and
.json.erb files, leaving out files git ignores) or a quoted glob such as
'app/**/*.rb'. With no PATH, formats stdin to stdout.

ERB files are formatted with syntax_tree's ERB plugin, except that <script>
and <style> bodies, and whole .js.erb, .css.erb and .json.erb files, go
through oxfmt (with the .oxfmtrc.json and node_modules/.bin/oxfmt, or
OXFMT_BIN, of the git repository's root).

Modes:
    -w, --write                  Format files in place (the default)
    -c, --check                  Check that files are formatted; exit 1 if not
    -l, --list-different         Print the paths of files that are not formatted

Options:
        --ignore-files=GLOB      Skip files, and directories, matching GLOB.
                                 Repeatable
        --plugins=NAME[,NAME]    syntax_tree plugins: plugin/single_quotes,
                                 plugin/trailing_comma, plugin/disable_auto_ternary
                                 (erb is accepted; ERB is always formatted)
        --print-width=N          The maximum line width (default 100)
        --target-ruby-version=V  The Ruby version to format for
        --config=PATH            Read options from PATH instead of .streerc
    -e SCRIPT                    Format an inline script
    -h, --help                   Show this help
    -v, --version                Show the version

.streerc holds one option per line, as for syntax_tree, and the command line
adds to it. syntax_tree's commands (`oxtree check FILE`, `oxtree write
FILE`, ...) work too; `oxtree help` lists them.

Exit status: 0 when everything is formatted, 1 when files are not (--check,
--list-different), 2 on errors.
";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Write,
    Check,
    ListDifferent,
}

/// Whether `argv` is for this command line rather than `stree`'s: it starts
/// with an option, or with a path or glob that isn't one of `stree`'s
/// actions. Anything else stays `stree`'s, which prints its help for an
/// unknown action.
pub fn handles(argv: &[String], cwd: &Path) -> bool {
    let Some(first) = argv.first() else {
        return false;
    };
    if STREE_ACTIONS.contains(&first.as_str()) {
        return false;
    }
    first.starts_with('-') || is_glob(first) || !glob::glob(first, cwd).is_empty()
}

fn is_glob(path: &str) -> bool {
    path.contains(['*', '?', '[', '{'])
}

pub fn run(argv: &[String], cwd: &Path, io: &mut Io) -> i32 {
    match run_flags(argv, cwd, io) {
        Ok(status) => status,
        Err(fatal) => {
            let _ = writeln!(io.stderr, "oxtree: {}", fatal.message);
            let _ = writeln!(io.stderr, "Run `oxtree --help` for usage.");
            2
        }
    }
}

fn run_flags(argv: &[String], cwd: &Path, io: &mut Io) -> Result<i32, Fatal> {
    let mut mode = None;
    let mut config = None;
    let mut hidden = None;
    let mut rest = Vec::new();
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        let chosen = match arg.as_str() {
            "-w" | "--write" => Mode::Write,
            "-c" | "--check" => Mode::Check,
            "-l" | "--list-different" => Mode::ListDifferent,
            "-h" | "--help" => {
                let _ = io.stdout.write_all(HELP.as_bytes());
                return Ok(0);
            }
            "-v" | "--version" => {
                let _ = writeln!(io.stdout, "{}", super::version());
                return Ok(0);
            }
            "--config" => {
                let path = args.next().ok_or_else(|| {
                    Fatal::new(
                        "OptionParser::MissingArgument",
                        "missing argument: --config",
                    )
                })?;
                config = Some(path.clone());
                continue;
            }
            // Hidden, for script/compare_with_stree: print the selected
            // files, or every one's formatted output as JSON, and stop.
            "--list-files" | "--emit-json" => {
                hidden = Some(arg.clone());
                continue;
            }
            "--" => {
                rest.push(arg.clone());
                rest.extend(args.by_ref().cloned());
                break;
            }
            _ => {
                match arg.strip_prefix("--config=") {
                    Some(path) => config = Some(path.to_string()),
                    None => rest.push(arg.clone()),
                }
                continue;
            }
        };
        if mode.is_some_and(|m| m != chosen) {
            return Err(Fatal::new(
                "OptionParser::InvalidOption",
                "only one of --write, --check and --list-different can be given",
            ));
        }
        mode = Some(chosen);
    }
    let mode = mode.unwrap_or(Mode::Write);

    let mut all = config_arguments(config.as_deref(), cwd)?;
    all.extend(rest);
    let mut options = Options {
        print_width: DEFAULT_PRINT_WIDTH,
        ..Options::default()
    };
    let paths = options.parse(all)?;

    if let Some(hidden) = hidden {
        let files = files(&paths, &options, cwd)?;
        if hidden == "--list-files" {
            for file in &files {
                let _ = writeln!(io.stdout, "{file}");
            }
        } else {
            emit_json(&files, &options, cwd, &oxfmt(cwd), io);
        }
        return Ok(0);
    }

    let mut queue = Vec::new();
    if paths.is_empty() && options.scripts.is_empty() {
        let mut buf = Vec::new();
        let _ = io.stdin.read_to_end(&mut buf);
        queue.push(Item::Stdin(String::from_utf8_lossy(&buf).into_owned()));
    } else {
        for file in files(&paths, &options, cwd)? {
            queue.push(Item::File(file));
        }
        for script in &options.scripts {
            queue.push(Item::Script(script.clone()));
        }
    }

    let oxfmt = oxfmt(cwd);
    // Largest first, so a few big files don't finish last on one thread.
    let mut order: Vec<(u64, usize)> = queue
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let size = match item {
                Item::File(path) => std::fs::metadata(cwd.join(path)).map_or(0, |m| m.len()),
                _ => 0,
            };
            (size, index)
        })
        .collect();
    order.sort_by_key(|&(size, index)| (std::cmp::Reverse(size), index));
    let done: Vec<(usize, ItemOutput)> = order
        .par_iter()
        .with_max_len(1)
        .map(|&(_, index)| {
            let item = &queue[index];
            let action = match (mode, item) {
                (Mode::Write, Item::File(_)) => Action::Write,
                // Nothing to write back to: print the formatted source.
                (Mode::Write, _) => Action::Format,
                (Mode::Check | Mode::ListDifferent, _) => Action::Check,
            };
            (index, run_item(action, item, &options, cwd, Some(&oxfmt)))
        })
        .collect();
    let mut outputs: Vec<Option<ItemOutput>> = queue.iter().map(|_| None).collect();
    for (index, output) in done {
        outputs[index] = Some(output);
    }

    let (mut unformatted, mut errored) = (false, false);
    for (item, output) in queue.iter().zip(outputs.into_iter().flatten()) {
        if let Some(warning) = &output.warning {
            let _ = writeln!(io.stderr, "{warning}");
        }
        if mode == Mode::ListDifferent && output.unformatted {
            let _ = writeln!(io.stdout, "{}", item.filepath());
        } else {
            let _ = io.stdout.write_all(&output.stdout);
            let _ = io.stderr.write_all(&output.stderr);
        }
        unformatted |= output.unformatted;
        errored |= output.errored && !output.unformatted;
    }
    if mode == Mode::Check {
        if unformatted || errored {
            let _ = writeln!(
                io.stderr,
                "The listed files did not match the expected format."
            );
        } else {
            let _ = writeln!(io.stdout, "All files matched expected format.");
        }
    }
    Ok(if errored {
        2
    } else if unformatted {
        1
    } else {
        0
    })
}

/// The files `paths` name, in order and without duplicates, less the
/// ignored ones.
fn files(paths: &[String], options: &Options, cwd: &Path) -> Result<Vec<String>, Fatal> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        let matches = glob::glob(path, cwd);
        if matches.is_empty() && !is_glob(path) {
            return Err(Fatal::new(
                "Errno::ENOENT",
                format!("No such file or directory - {path}"),
            ));
        }
        for found in matches {
            let expanded = if cwd.join(&found).is_dir() {
                directory_files(&found, cwd)
                    .into_iter()
                    .filter(|file| formats(file))
                    .collect()
            } else {
                vec![found]
            };
            for file in expanded {
                if !ignored(&file, &options.ignore_files) && seen.insert(file.clone()) {
                    out.push(file);
                }
            }
        }
    }
    Ok(out)
}

/// Whether a file found in a directory is one oxtree formats.
fn formats(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    RUBY_FILENAMES.contains(&name)
        || RUBY_EXTENSIONS.contains(&extname(name))
        || ERB_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

/// The embedded ERB formatter's oxfmt: configured from, and run in, the
/// git repository's root (else `cwd`), with `OXFMT_BIN` or
/// `node_modules/.bin/oxfmt` as the executable.
fn oxfmt(cwd: &Path) -> Oxfmt {
    let root = git_toplevel(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let binary = std::env::var("OXFMT_BIN")
        .ok()
        .filter(|bin| !bin.is_empty())
        .map(PathBuf::from);
    Oxfmt::new(&root, binary, Backend::Library)
}

fn git_toplevel(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let path = String::from_utf8(output.stdout).ok()?;
    Some(PathBuf::from(path.trim_end_matches(['\n', '\r'])))
}

/// Hidden `--emit-json`: `{path: formatted}` for every selected file, with
/// `"ERROR Class: message"` (the message's first line, as stree_new prints
/// it) for the ones that fail. Never writes.
fn emit_json(files: &[String], options: &Options, cwd: &Path, oxfmt: &Oxfmt, io: &mut Io) {
    let results: Vec<(String, String)> = files
        .par_iter()
        .with_max_len(1)
        .map(|file| {
            let text = match format_file(file, options, cwd, oxfmt) {
                Ok(formatted) => formatted,
                Err((class, message)) => {
                    let first = message.split_inclusive('\n').next().unwrap_or("").trim();
                    format!("ERROR {class}: {first}")
                }
            };
            (file.clone(), text)
        })
        .collect();
    let map: serde_json::Map<String, serde_json::Value> = results
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect();
    let _ = writeln!(io.stdout, "{}", serde_json::Value::Object(map));
}

/// A file formatted as the flag CLI formats it, or the error's class and
/// message.
fn format_file(
    file: &str,
    options: &Options,
    cwd: &Path,
    oxfmt: &Oxfmt,
) -> Result<String, (&'static str, String)> {
    let handler = handler_for(extname(file), &options.plugins, Some(oxfmt));
    let bytes =
        std::fs::read(cwd.join(file)).map_err(|e| ("SystemCallError", io_message(&e, file)))?;
    let source = match handler {
        Handler::Ruby => read_ruby(bytes),
        Handler::Erb | Handler::Embedded(_) => read_utf8(bytes),
    };
    source
        .and_then(|source| format_with(handler, &source.text, options, file))
        .map_err(|failure| match failure {
            Failure::Parse(e) => (PARSE_ERROR_CLASS, e.message),
            Failure::Other { class, message } => (class, message),
            Failure::Unformatted | Failure::NonIdempotent => ("StandardError", String::new()),
        })
}

/// `--ignore-files`: `stree`'s plain `fnmatch` against the path, or against
/// any directory it is in.
fn ignored(file: &str, globs: &[String]) -> bool {
    let file = file.strip_prefix("./").unwrap_or(file);
    globs.iter().any(|glob| {
        let glob = glob.strip_suffix('/').unwrap_or(glob);
        fnmatch(glob, file, 0)
            || file
                .match_indices('/')
                .any(|(i, _)| fnmatch(glob, &file[..i], 0))
    })
}

/// The files under `dir`, spelled from `dir`: what git tracks or leaves
/// untracked but not ignored, or every file outside a git repository. Like
/// `Dir.glob("**/*")` and RuboCop, it skips dot-directories.
fn directory_files(dir: &str, cwd: &Path) -> Vec<String> {
    let root = cwd.join(dir);
    let prefix = match dir.trim_end_matches('/') {
        "" if dir.starts_with('/') => "/".to_string(),
        "" | "." => String::new(),
        dir => format!("{dir}/"),
    };
    let listed = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(&root)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success());
    let relative = match listed {
        Some(output) => String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|f| {
                !f.is_empty()
                    && !f.split('/').rev().skip(1).any(|d| d.starts_with('.'))
                    && root.join(f).is_file()
            })
            .map(str::to_string)
            .collect(),
        None => {
            let mut files = Vec::new();
            walk(&root, "", &mut files);
            files
        }
    };
    relative
        .into_iter()
        .map(|f| format!("{prefix}{f}"))
        .collect()
}

/// Every file under `dir`, sorted, skipping dot-directories.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            if !name.starts_with('.') {
                walk(&path, &format!("{prefix}{name}/"), out);
            }
        } else {
            out.push(format!("{prefix}{name}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_globs_match_files_and_their_directories() {
        let globs = |g: &[&str]| g.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(ignored("db/schema.rb", &globs(&["db"])));
        assert!(ignored("./db/migrate/1.rb", &globs(&["db/"])));
        assert!(ignored("vendor/a/b.rb", &globs(&["vendor/**/*"])));
        assert!(ignored("tmp/x.rb", &globs(&["tmp/*.rb"])));
        assert!(!ignored("app/db.rb", &globs(&["db"])));
        assert!(!ignored("dbx/a.rb", &globs(&["db"])));
    }

    #[test]
    fn directories_expand_to_ruby_and_erb_files() {
        for (file, expected) in [
            ("app/a.rb", true),
            ("lib/tasks/a.rake", true),
            ("x.gemspec", true),
            ("config.ru", true),
            ("Gemfile", true),
            ("sub/Rakefile", true),
            ("Gemfile.lock", false),
            ("app/views/a.html.erb", true),
            ("app/views/a.js.erb", true),
            ("app/views/a.css.erb", true),
            ("app/views/a.json.erb", true),
            ("app/views/a.text.erb", false),
            ("app/views/a.erb", false),
            ("app/views/a.HTML.erb", false),
            ("README.md", false),
        ] {
            assert_eq!(formats(file), expected, "{file}");
        }
    }
}
