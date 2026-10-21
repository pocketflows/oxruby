//! Ports of syntax_tree's `test/cli_test.rb`, run in-process. Cases that
//! need syntax_tree's Ruby objects (`ast`, `doc`, `expr`, `json`, `match`,
//! `search`, `lsp`), custom Ruby handlers or plugin files, or stubbed
//! `SyntaxTree.format` don't apply to a binary and are left out; see
//! docs/agents/tree-cli.md.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Result {
    status: i32,
    stdio: String,
    stderr: String,
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh working directory (upstream runs in the repository, whose
/// `.streerc` doesn't exist).
fn tmpdir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("oxtree-cli-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_in(dir: &Path, args: &[&str], stdin: &str) -> Result {
    let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut input = stdin.as_bytes();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = {
        let mut io = Io {
            stdin: &mut input,
            stdout: &mut out,
            stderr: &mut err,
        };
        run(&argv, dir, &mut io)
    };
    Result {
        status,
        stdio: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

/// `run_cli(command, *args, contents:)`: the contents go in a `test-*.rb`
/// tempfile whose absolute path is the last argument.
fn run_cli_in(dir: &Path, command: &str, args: &[&str], contents: &str) -> Result {
    let file = dir.join("test-file.rb");
    std::fs::write(&file, contents).unwrap();
    let path = file.to_str().unwrap().to_string();
    let mut argv = vec![command];
    argv.extend_from_slice(args);
    argv.push(&path);
    run_in(dir, &argv, "")
}

fn run_cli(command: &str, args: &[&str], contents: &str) -> Result {
    run_cli_in(&tmpdir(), command, args, contents)
}

const TEST: &str = "test\n";

#[test]
fn test_check() {
    let result = run_cli("check", &[], TEST);
    assert!(result.stdio.contains("match"));
    assert_eq!(result.status, 0);
}

#[test]
fn test_check_unformatted() {
    let result = run_cli("check", &[], "foo");
    assert!(result.stderr.contains("expected"));
    assert_ne!(result.status, 0);
    assert!(result.stderr.starts_with("[\x1b[33mwarn\x1b[0m] /"));
}

#[test]
fn test_check_print_width() {
    let contents = format!("{} + {}\n", "a".repeat(40), "b".repeat(40));
    let result = run_cli("check", &["--print-width=100"], &contents);
    assert!(result.stdio.contains("match"));
}

#[test]
fn test_check_target_ruby_version() {
    let result = run_cli("check", &["--target-ruby-version=2.6.0"], TEST);
    assert!(result.stdio.contains("match"));
}

#[test]
fn test_debug() {
    let result = run_cli("debug", &[], TEST);
    assert!(result.stdio.contains("idempotently"));
}

#[test]
fn test_format() {
    let result = run_cli("format", &[], TEST);
    assert_eq!(result.stdio, "test\n");
}

#[test]
fn test_write() {
    let dir = tmpdir();
    let file = dir.join("test-file.test");
    std::fs::write(&file, "").unwrap();
    let path = file.to_str().unwrap();
    let result = run_in(&dir, &["write", path], "");
    assert!(result.stdio.contains(path));
}

#[test]
fn test_write_syntax_tree() {
    // Upstream asserts Ripper's "syntax error" text; oxtree reports Prism's
    // message (user decision), so only the failure and the report shape are
    // checked.
    let result = run_cli("write", &[], "<>");
    assert!(result.stderr.starts_with("Error: "), "{}", result.stderr);
    assert_ne!(result.status, 0);
}

#[test]
fn test_write_script() {
    let result = run_in(&tmpdir(), &["write", "-e", "1 + 2"], "");
    assert!(result.stdio.contains("script"));
    assert!(result.stderr.is_empty());
}

#[test]
fn test_write_stdin() {
    let result = run_in(&tmpdir(), &["write"], "1 + 2");
    assert!(result.stdio.contains("stdin"));
    assert!(result.stderr.is_empty());
}

#[test]
fn test_help() {
    let result = run_in(&tmpdir(), &["help"], "");
    assert!(result.stdio.contains("oxtree help"));
    assert_eq!(result.status, 0);
}

#[test]
fn test_help_default() {
    let result = run_in(&tmpdir(), &["foobar"], "");
    assert!(result.stderr.contains("oxtree help"));
    assert_ne!(result.status, 0);
}

#[test]
fn test_no_arguments() {
    let result = run_in(&tmpdir(), &["format"], "1+1");
    assert_eq!(result.stdio, "1 + 1\n");
}

#[test]
fn test_inline_script() {
    let result = run_in(&tmpdir(), &["format", "-e", "1+1"], "");
    assert_eq!(result.stdio, "1 + 1\n");
}

#[test]
fn test_multiple_inline_scripts() {
    let result = run_in(&tmpdir(), &["format", "-e", "1+1", "-e", "2+2"], "");
    let mut lines: Vec<&str> = result.stdio.lines().collect();
    lines.sort();
    assert_eq!(lines, ["1 + 1", "2 + 2"]);
}

#[test]
fn test_version() {
    let result = run_in(&tmpdir(), &["version"], "");
    assert!(result.stdio.contains(SYNTAX_TREE_VERSION));
}

// test_plugins/test_config_file load Ruby plugin files; here the plugins
// are the ones syntax_tree ships.
#[test]
fn test_plugins() {
    let result = run_in(
        &tmpdir(),
        &["format", "--plugins=plugin/single_quotes", "-e", "\"a\""],
        "",
    );
    assert_eq!(result.stdio, "'a'\n");
    let result = run_in(&tmpdir(), &["format", "--plugins=nope", "-e", "1"], "");
    assert_eq!(result.status, 1);
    assert!(
        result
            .stderr
            .contains("cannot load such file -- syntax_tree/nope")
    );
}

fn with_config_file(dir: &Path, contents: &str) {
    std::fs::write(dir.join(".streerc"), contents).unwrap();
}

#[test]
fn test_config_file() {
    let dir = tmpdir();
    with_config_file(&dir, "--print-width=100\n--plugins=plugin/single_quotes\n");
    let contents = format!("{} + {}\n", "a".repeat(40), "b".repeat(40));
    let result = run_cli_in(&dir, "format", &[], &contents);
    assert_eq!(result.stdio, contents);
    let result = run_in(&dir, &["format", "-e", "\"a\""], "");
    assert_eq!(result.stdio, "'a'\n");
}

#[test]
fn test_print_width_args_with_config_file() {
    let dir = tmpdir();
    with_config_file(&dir, "--print-width=100");
    let contents = format!("{} + {}\n", "a".repeat(40), "b".repeat(40));
    let result = run_cli_in(&dir, "check", &[], &contents);
    assert!(result.stdio.contains("match"));
}

#[test]
fn test_print_width_args_with_config_file_override() {
    let dir = tmpdir();
    with_config_file(&dir, "--print-width=100");
    let contents = format!("{} + {}\n", "a".repeat(40), "b".repeat(40));
    let result = run_cli_in(&dir, "check", &["--print-width=82"], &contents);
    assert!(result.stderr.contains("expected"));
    assert_ne!(result.status, 0);
}

#[test]
fn test_plugin_args_with_config_file() {
    let dir = tmpdir();
    with_config_file(&dir, "--plugins=plugin/single_quotes");
    let result = run_in(
        &dir,
        &[
            "format",
            "--plugins=plugin/trailing_comma",
            "-e",
            &format!("foo(\"{}\", {})", "a".repeat(40), "b".repeat(40)),
        ],
        "",
    );
    assert_eq!(
        result.stdio,
        format!("foo(\n  '{}',\n  {},\n)\n", "a".repeat(40), "b".repeat(40))
    );
}

#[test]
fn test_config_file_custom_path() {
    let dir = tmpdir();
    let config = tmpdir().join("custom.streerc");
    std::fs::write(
        &config,
        "--print-width=80\n--plugins=plugin/single_quotes\n",
    )
    .unwrap();
    let flag = format!("--config={}", config.display());
    let result = run_in(&dir, &["format", &flag, "-e", "\"a\""], "");
    assert_eq!(result.stdio, "'a'\n");
}

#[test]
fn test_config_file_custom_path_space_separated() {
    let dir = tmpdir();
    let config = tmpdir().join("custom.streerc");
    std::fs::write(&config, "--plugins=plugin/single_quotes\n").unwrap();
    let path = config.to_str().unwrap();
    let result = run_in(&dir, &["format", "--config", path, "-e", "\"a\""], "");
    assert_eq!(result.stdio, "'a'\n");
}

#[test]
fn test_config_file_nonexistent_path() {
    let result = run_in(
        &tmpdir(),
        &["format", "--config=/nonexistent/path.streerc", "-e", "1"],
        "",
    );
    assert_eq!(result.status, 1);
    assert!(
        result
            .stderr
            .contains("Invalid configuration file: /nonexistent/path.streerc")
    );
    assert!(result.stderr.contains("ArgumentError"));
}

// Beyond cli_test.rb: behaviour read off cli.rb.

#[test]
fn ignore_files_strip_quotes_and_use_plain_fnmatch() {
    let dir = tmpdir();
    std::fs::create_dir_all(dir.join("db/migrate")).unwrap();
    std::fs::write(dir.join("db/migrate/a.rb"), "foo").unwrap();
    std::fs::write(dir.join("b.rb"), "bar\n").unwrap();
    let result = run_in(&dir, &["check", "--ignore-files='db/*'", "**/*.rb"], "");
    assert_eq!(result.status, 0, "{}", result.stderr);
    let result = run_in(&dir, &["check", "**/*.rb"], "");
    assert_eq!(result.status, 1);
    assert_eq!(
        result.stderr,
        "[\x1b[33mwarn\x1b[0m] db/migrate/a.rb\nThe listed files did not match the expected format.\n"
    );
}

#[test]
fn unmatched_globs_succeed_without_reading_stdin() {
    let result = run_in(&tmpdir(), &["check", "nothing/*.rb"], "foo");
    assert_eq!(result.status, 0);
    assert_eq!(result.stdio, "All files matched expected format.\n");
}

#[test]
fn write_reports_unchanged_files_in_gray() {
    let dir = tmpdir();
    std::fs::write(dir.join("a.rb"), "a\n").unwrap();
    std::fs::write(dir.join("b.rb"), "b  +  1").unwrap();
    let result = run_in(&dir, &["write", "a.rb", "b.rb"], "");
    let lines: Vec<&str> = result.stdio.lines().collect();
    assert!(lines[0].starts_with("\x1b[38;5;102ma.rb\x1b[0m ") && lines[0].ends_with("ms"));
    assert!(lines[1].starts_with("b.rb ") && lines[1].ends_with("ms"));
    assert_eq!(
        std::fs::read_to_string(dir.join("b.rb")).unwrap(),
        "b + 1\n"
    );
}

#[test]
fn parse_errors_are_highlighted() {
    let dir = tmpdir();
    std::fs::write(dir.join("a.rb"), "foo\nbar(\nbaz\n").unwrap();
    let result = run_in(&dir, &["check", "a.rb"], "");
    assert_eq!(result.status, 1);
    let lines: Vec<&str> = result.stderr.lines().collect();
    assert_eq!(lines[0], "[\x1b[33mwarn\x1b[0m] a.rb");
    assert!(lines[1].starts_with("Error: "));
    // Three lines of context, the `>` marker and the caret line.
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("\x1b[1;31m>\x1b[0m \x1b[38;5;102m"))
    );
    assert!(lines.iter().any(|l| l.ends_with("\x1b[1;31m^\x1b[0m")));
    assert_eq!(
        *lines.last().unwrap(),
        "The listed files did not match the expected format."
    );
}

#[test]
fn erb_needs_the_plugin() {
    let dir = tmpdir();
    std::fs::write(dir.join("a.html.erb"), "<div>\n<%= foo %>\n</div>\n").unwrap();
    let result = run_in(&dir, &["format", "a.html.erb"], "");
    assert_eq!(result.status, 1);
    let result = run_in(&dir, &["format", "--plugins=erb", "a.html.erb"], "");
    assert_eq!(result.status, 0);
    assert_eq!(result.stdio, "<div><%= foo %></div>\n");
    let result = run_in(
        &dir,
        &[
            "format",
            "--plugins=erb,plugin/single_quotes",
            "-e",
            "<%= \"a\" %>",
            "--extension=erb",
        ],
        "",
    );
    assert_eq!(result.stdio, "<%= 'a' %>\n");
}

#[test]
fn option_errors_exit_1() {
    for (args, message) in [
        (
            &["format", "--nope"][..],
            "invalid option: --nope (OptionParser::InvalidOption)",
        ),
        (
            &["format", "--p=1"][..],
            "ambiguous option: --p=1 (OptionParser::AmbiguousOption)",
        ),
        (
            &["format", "--print-width=x"][..],
            "invalid argument: --print-width=x",
        ),
        (
            &["format", "--print-width"][..],
            "missing argument: --print-width",
        ),
    ] {
        let result = run_in(&tmpdir(), args, "1");
        assert_eq!(result.status, 1, "{args:?}");
        assert!(
            result.stderr.contains(message),
            "{args:?}: {}",
            result.stderr
        );
    }
    let result = run_in(&tmpdir(), &["format", "--print=0x10", "-e", "1"], "");
    assert_eq!(result.status, 0);
}

#[test]
fn ruby_integers() {
    assert_eq!(parse_ruby_integer("100"), Some(100));
    assert_eq!(parse_ruby_integer("010"), Some(8));
    assert_eq!(parse_ruby_integer("0x10"), Some(16));
    assert_eq!(parse_ruby_integer("1_000"), Some(1000));
    assert_eq!(parse_ruby_integer("-5"), Some(-5));
    assert_eq!(parse_ruby_integer(" 5"), None);
    assert_eq!(parse_ruby_integer("0"), Some(0));
}

#[test]
fn extnames() {
    assert_eq!(extname("a/b.html.erb"), ".erb");
    assert_eq!(extname("Rakefile"), "");
    assert_eq!(extname(".erb"), "");
    assert_eq!(extname("a.d/b"), "");
    assert_eq!(extname("x.rb"), ".rb");
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn stree_new_check_and_write() {
    let dir = tmpdir();
    git(&dir, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(dir.join("db/x")).unwrap();
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(dir.join("ok.rb"), "a\n").unwrap();
    std::fs::write(dir.join("bad.rb"), "a  +  1").unwrap();
    std::fs::write(dir.join("broken.rb"), "def").unwrap();
    std::fs::write(dir.join("db/x/skip.rb"), "a  +  1").unwrap();
    std::fs::write(dir.join("notes.txt"), "a  +  1").unwrap();
    std::fs::write(dir.join("app/v.html.erb"), "<div>\n<%= foo %>\n</div>\n").unwrap();
    std::fs::write(dir.join(".streerc"), "--ignore-files=db/**/*.rb\n").unwrap();
    git(&dir, &["add", "ok.rb", "notes.txt"]);

    let result = run_in(&dir, &["stree-new", "--list"], "");
    let mut listed: Vec<&str> = result.stdio.lines().collect();
    listed.sort();
    assert_eq!(listed, ["app/v.html.erb", "bad.rb", "broken.rb", "ok.rb"]);

    let result = run_in(&dir, &["stree-new", "check"], "");
    assert_eq!(result.status, 1);
    let lines: Vec<&str> = result.stdio.lines().collect();
    // Largest first: app/v.html.erb (24 bytes), bad.rb (7), broken.rb (3), ok.rb (2).
    assert_eq!(lines[0], "[warn] app/v.html.erb");
    assert_eq!(lines[1], "[warn] bad.rb");
    assert!(
        lines[2].starts_with("[error] broken.rb: SyntaxTree::Parser::ParseError: "),
        "{}",
        lines[2]
    );
    assert_eq!(lines[3], "");
    assert!(lines[4].starts_with("check completed with 3 file(s) reporting errors ("));

    let result = run_in(&dir, &["stree-new", "write"], "");
    assert_eq!(result.status, 1);
    let lines: Vec<&str> = result.stdio.lines().collect();
    assert_eq!(lines[0], "app/v.html.erb");
    assert_eq!(lines[1], "bad.rb");
    assert!(lines[4].starts_with("write completed with 1 file(s) reporting errors ("));
    assert_eq!(
        std::fs::read_to_string(dir.join("bad.rb")).unwrap(),
        "a + 1\n"
    );

    std::fs::remove_file(dir.join("broken.rb")).unwrap();
    let result = run_in(&dir, &["stree-new", "check"], "");
    assert_eq!(result.status, 0, "{}", result.stdio);
    let processes = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(3);
    assert!(result.stdio.starts_with(&format!(
        "\ncheck passed for 3 files across {processes} processes ("
    )));

    git(&dir, &["add", "."]);
    git(
        &dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    );
    let result = run_in(&dir, &["stree-new", "check:modified"], "");
    assert!(
        result.stdio.starts_with("No files to check ("),
        "{}",
        result.stdio
    );
    std::fs::write(dir.join("new.rb"), "x  =  1").unwrap();
    let result = run_in(&dir, &["stree-new", "check", "--modified"], "");
    assert!(
        result.stdio.starts_with("[warn] new.rb\n"),
        "{}",
        result.stdio
    );
}

// User decision: without oxfmt, Tailwind classes stay unsorted with a
// warning instead of an OxfmtMissingError.
#[cfg(feature = "oxc")]
#[test]
fn stree_new_warns_when_tailwind_needs_oxfmt() {
    let dir = tmpdir();
    git(&dir, &["init", "-q", "-b", "main"]);
    std::fs::write(
        dir.join(".oxfmtrc.json"),
        r#"{"experimentalTailwindcss": {"functions": ["cn"]}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("a.html.erb"),
        "<script>\n  const a = cn(\"p-4 flex\");\n</script>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("b.html.erb"),
        "<script>\n  const b = 1;\n</script>\n",
    )
    .unwrap();
    let result = run_in(&dir, &["stree-new", "check"], "");
    assert_eq!(result.status, 0, "{}", result.stdio);
    assert_eq!(
        result.stderr,
        "[warn] a.html.erb: oxfmt not found; Tailwind classes left unsorted\n"
    );
}
