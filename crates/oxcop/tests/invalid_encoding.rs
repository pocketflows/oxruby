//! Sources whose bytes aren't valid in their encoding fail as RuboCop
//! 1.91.0's do: `Parser::Source::Buffer#source=` raises, rubocop-ast's
//! `ProcessedSource#parse` keeps the error as `parser_error`, and Lint/Syntax
//! reports it as a global fatal offense. No other cop runs and nothing is
//! corrected. The expected outputs were measured with RuboCop 1.91.0 on
//! Ruby 3.3 (see docs/agents/utf8.md).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FATAL: &str = "1:1: F: Lint/Syntax: Invalid byte sequence in utf-8.\n";

/// A directory of fixtures with its own (empty) config, so the repository's
/// `.rubocop.yml` doesn't apply.
fn fixture_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".rubocop.yml"), "AllCops:\n  NewCops: disable\n").unwrap();
    dir
}

fn oxcop(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oxcop"));
    command
        .current_dir(dir)
        .args(["--cache", "false"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    {
        use std::io::Write;
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(stdin.unwrap_or_default()).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn emacs(dir: &Path, file: &str, bytes: &[u8], extra: &[&str]) -> (String, Option<i32>) {
    std::fs::write(dir.join(file), bytes).unwrap();
    let mut args = vec!["--format", "emacs"];
    args.extend_from_slice(extra);
    args.push(file);
    let output = oxcop(dir, &args, None);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let prefix = format!("{}:", dir.join(file).display());
    (stdout.replace(&prefix, ""), output.status.code())
}

const BARE: &[u8] = b"x = \"abc\"\ny = \xff\n";
const IN_STRING: &[u8] = b"x = 'a\xffb'\n# comment \xfe here\ny = \"double\"\n";
const ONLY: [&str; 2] = ["--only", "Lint/Syntax,Style/StringLiterals"];

#[test]
fn invalid_utf8_is_a_fatal_lint_syntax_offense() {
    let dir = fixture_dir("invalid_utf8");
    assert_eq!(
        emacs(&dir, "bare.rb", BARE, &ONLY),
        (FATAL.to_owned(), Some(1))
    );
    assert_eq!(
        emacs(&dir, "in_string.rb", IN_STRING, &ONLY),
        (FATAL.to_owned(), Some(1))
    );
    // A UTF-8 magic comment or a BOM changes nothing.
    let utf8_comment = b"# encoding: utf-8\nx = 'a\xffb'\ny = \"double\"\n";
    assert_eq!(
        emacs(&dir, "comment.rb", utf8_comment, &ONLY),
        (FATAL.to_owned(), Some(1))
    );
    let bom = b"\xef\xbb\xbfx = \"\xff\"\n";
    assert_eq!(
        emacs(&dir, "bom.rb", bom, &ONLY),
        (FATAL.to_owned(), Some(1))
    );
    // Every cop: still only Lint/Syntax.
    assert_eq!(
        emacs(&dir, "all.rb", IN_STRING, &[]),
        (FATAL.to_owned(), Some(1))
    );
}

#[test]
fn lint_syntax_runs_with_any_only() {
    let dir = fixture_dir("only");
    let only = ["--only", "Style/StringLiterals"];
    assert_eq!(
        emacs(&dir, "bare.rb", BARE, &only),
        (FATAL.to_owned(), Some(1))
    );
    let (stdout, status) = emacs(&dir, "syntax.rb", b"x = 'a'\ndef\n", &only);
    assert!(stdout.starts_with("2:1: F: Lint/Syntax: "), "{stdout}");
    assert_eq!(status, Some(1));
}

#[test]
fn syntax_checking_cannot_be_turned_off() {
    let dir = fixture_dir("except");
    std::fs::write(dir.join("bare.rb"), BARE).unwrap();
    for except in ["Lint/Syntax", "Syntax"] {
        let output = oxcop(&dir, &["--except", except, "bare.rb"], None);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, b"Syntax checking cannot be turned off.\n");
    }
}

#[test]
fn formatters_show_a_global_offense() {
    let dir = fixture_dir("formatters");
    std::fs::write(dir.join("bare.rb"), BARE).unwrap();
    let path = dir.join("bare.rb").display().to_string();
    let run = |format: &str| {
        let output = oxcop(
            &dir,
            &["--only", "Lint/Syntax", "--format", format, "bare.rb"],
            None,
        );
        assert_eq!(output.status.code(), Some(1));
        String::from_utf8(output.stdout)
            .unwrap()
            .replace(&path, "bare.rb")
    };
    assert_eq!(
        run("progress"),
        "Inspecting 1 file\nF\n\nOffenses:\n\nbare.rb:1:1: F: Lint/Syntax: Invalid byte sequence in utf-8.\n\n1 file inspected, 1 offense detected\n"
    );
    assert_eq!(
        run("simple"),
        "== bare.rb ==\nF:  1:  1: Lint/Syntax: Invalid byte sequence in utf-8.\n\n1 file inspected, 1 offense detected\n"
    );
    assert_eq!(
        run("clang"),
        "bare.rb:1:1: F: Lint/Syntax: Invalid byte sequence in utf-8.\n\n1 file inspected, 1 offense detected\n"
    );
    let json: serde_json::Value = serde_json::from_str(&run("json")).unwrap();
    assert_eq!(
        json["files"][0]["offenses"],
        serde_json::json!([{
            "severity": "fatal",
            "message": "Invalid byte sequence in utf-8.",
            "cop_name": "Lint/Syntax",
            "corrected": false,
            "correctable": false,
            "location": {
                "start_line": 1, "start_column": 1, "last_line": 1, "last_column": 1,
                "length": 0, "line": 1, "column": 1
            }
        }])
    );
}

#[test]
fn nothing_is_corrected() {
    let dir = fixture_dir("autocorrect");
    for flag in ["-a", "-A"] {
        assert_eq!(
            emacs(&dir, "file.rb", IN_STRING, &[flag]),
            (FATAL.to_owned(), Some(1))
        );
        assert_eq!(std::fs::read(dir.join("file.rb")).unwrap(), IN_STRING);
    }
}

#[test]
fn stdin() {
    let dir = fixture_dir("stdin");
    let output = oxcop(
        &dir,
        &[ONLY[0], ONLY[1], "--format", "emacs", "--stdin", "stdin.rb"],
        Some(IN_STRING),
    );
    assert_eq!(output.status.code(), Some(1));
    let path = dir.join("stdin.rb").display().to_string();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{path}:{FATAL}")
    );

    // With autocorrect, the source is echoed back unchanged.
    let output = oxcop(
        &dir,
        &[
            ONLY[0], ONLY[1], "--format", "emacs", "-a", "--stdin", "stdin.rb",
        ],
        Some(IN_STRING),
    );
    assert_eq!(output.status.code(), Some(1));
    let mut expected = format!("{path}:{FATAL}====================\n").into_bytes();
    expected.extend_from_slice(IN_STRING);
    assert_eq!(output.stdout, expected);
}

#[test]
fn other_encodings() {
    let dir = fixture_dir("encodings");
    let string_literal = "3:5: C: [Correctable] Style/StringLiterals: Prefer single-quoted strings when you don't need string interpolation or special symbols.\n";
    for (file, bytes) in [
        (
            "latin1.rb",
            &b"# encoding: ISO-8859-1\nx = 'caf\xe9'\ny = \"double\"\n"[..],
        ),
        (
            "binary.rb",
            b"# encoding: binary\nx = 'caf\xe9'\ny = \"double\"\n",
        ),
        (
            "sjis.rb",
            b"# encoding: Shift_JIS\nx = '\x82\xa0'\ny = \"double\"\n",
        ),
    ] {
        assert_eq!(
            emacs(&dir, file, bytes, &ONLY),
            (string_literal.to_owned(), Some(1)),
            "{file}"
        );
    }
    for (file, bytes, message) in [
        (
            "sjis_invalid.rb",
            &b"# encoding: Shift_JIS\nx = \"\x82\xa0\"\ny = \"\xff\xfe\"\n"[..],
            r#""\xff" on shift_jis."#,
        ),
        (
            "eucjp.rb",
            b"# encoding: EUC-JP\nx = \"\xa4\xa2\x8f\"\n",
            r#""\x8f" followed by "\"" on euc-jp."#,
        ),
        (
            "eucjp_incomplete.rb",
            b"# encoding: EUC-JP\nx = 1 # \xa4",
            r#"Incomplete "\xa4" on euc-jp."#,
        ),
        (
            "usascii.rb",
            b"# encoding: us-ascii\nx = \"caf\xc3\xa9\"\n",
            r#""\xc3" on us-ascii."#,
        ),
        (
            "unknown.rb",
            b"# encoding: bogus-enc\nx = \"a\"\n",
            "Unknown encoding name - bogus-enc.",
        ),
    ] {
        assert_eq!(
            emacs(&dir, file, bytes, &ONLY),
            (format!("1:1: F: Lint/Syntax: {message}\n"), Some(1)),
            "{file}"
        );
    }
}
