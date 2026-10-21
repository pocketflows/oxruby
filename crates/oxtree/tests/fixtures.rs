//! Runs syntax_tree's formatting fixtures (vendor/syntax_tree/test/fixtures)
//! the way `test/formatting_test.rb` does, plus an idempotency check on every
//! expected output. Set `OXTREE_FIXTURE=name` to run one fixture file.

use std::path::PathBuf;

use oxtree::format::Options;

struct Fixture {
    name: String,
    source: String,
    formatted: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/syntax_tree/test/fixtures")
}

/// `Fixtures.each_fixture` from test_helper.rb: cases are separated by lines
/// matching `/%(?: # (.+?))?\n/`, and `-\n` splits source from output.
fn each_fixture() -> Vec<Fixture> {
    let filter = std::env::var("OXTREE_FIXTURE").ok();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("fixtures directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rb"))
        .collect();
    paths.sort();

    let mut fixtures = Vec::new();
    for path in paths {
        let fixture = path.file_stem().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|f| f != &fixture) {
            continue;
        }
        let contents = std::fs::read_to_string(&path).unwrap();
        let mut cases: Vec<(Option<String>, Vec<&str>)> = Vec::new();
        for line in contents.split_inclusive('\n') {
            if let Some(comment) = delimiter(line) {
                cases.push((comment, Vec::new()));
            } else if let Some(case) = cases.last_mut() {
                case.1.push(line);
            }
        }
        for (index, (comment, lines)) in cases.into_iter().enumerate() {
            // Version gates (`% # >= 3.1.0`): oxtree targets Ruby 3.3.
            if let Some(comment) = &comment
                && let Some(version) = comment.strip_prefix(">= ")
                && version_gt(version, "3.3.11")
            {
                continue;
            }
            let joined: String = lines.concat();
            let (source, formatted) = match joined.split_once("-\n") {
                Some((source, rest)) => {
                    // `split("-\n")` keeps only the first two pieces when
                    // there is no further delimiter.
                    // ... and drops trailing empty pieces, so an empty output
                    // means "same as the source".
                    let formatted = rest.split("-\n").next().unwrap_or("");
                    let formatted = if formatted.is_empty() {
                        source
                    } else {
                        formatted
                    };
                    (source.to_string(), formatted.to_string())
                }
                None => (joined.clone(), joined.clone()),
            };
            fixtures.push(Fixture {
                name: format!("{fixture}_{index}"),
                source,
                formatted,
            });
        }
    }
    fixtures
}

fn delimiter(line: &str) -> Option<Option<String>> {
    // The regexp isn't anchored, but fixtures only use it at line start.
    let rest = line.strip_prefix('%')?;
    if rest == "\n" {
        return Some(None);
    }
    let comment = rest.strip_prefix(" # ")?.strip_suffix('\n')?;
    Some(Some(comment.to_string()))
}

fn version_gt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(a) > parse(b)
}

fn run(check: impl Fn(&Fixture) -> Option<String>) {
    let fixtures = each_fixture();
    assert!(!fixtures.is_empty(), "no fixtures found");
    let mut failures = Vec::new();
    for fixture in &fixtures {
        if let Some(message) = check(fixture) {
            failures.push(format!("{}:\n{message}", fixture.name));
        }
    }
    println!(
        "{} passed, {} failed",
        fixtures.len() - failures.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n")
    );
}

fn format(source: &str) -> Result<String, String> {
    oxtree::format(source, 80, Options::default()).map_err(|e| e.to_string())
}

#[test]
fn formatting_fixtures() {
    run(|fixture| match format(&fixture.source) {
        Ok(actual) if actual == fixture.formatted => None,
        Ok(actual) => Some(format!(
            "--- expected\n{}--- actual\n{}",
            fixture.formatted, actual
        )),
        Err(e) => Some(format!("parse error: {e}")),
    });
}

#[test]
fn formatting_fixtures_are_idempotent() {
    run(|fixture| match format(&fixture.formatted) {
        Ok(actual) if actual == fixture.formatted => None,
        Ok(actual) => Some(format!(
            "--- expected\n{}--- actual\n{}",
            fixture.formatted, actual
        )),
        Err(e) => Some(format!("parse error: {e}")),
    });
}

// The remaining tests of test/formatting_test.rb.

#[test]
fn format_class_level() {
    assert_eq!(format("1+1").unwrap(), "1 + 1\n");
}

#[test]
fn stree_ignore() {
    let source = "# stree-ignore\n1+1\n";
    assert_eq!(format(source).unwrap(), source);
}

#[test]
fn formatting_with_different_indentation_level() {
    let source = "def foo\n  puts \"a\"\nend\n";
    assert_eq!(format(source).unwrap(), source);
    let ast = oxtree::parser::parse(source).unwrap();
    let with = |level| oxtree::format::format(&ast, 80, level, Options::default());
    assert_eq!(with(2).trim_end(), "  def foo\n    puts \"a\"\n  end");
    assert_eq!(with(4).trim_end(), "    def foo\n      puts \"a\"\n    end");
    assert_eq!(
        with(6).trim_end(),
        "      def foo\n        puts \"a\"\n      end"
    );
}

// test/quotes_test.rb

#[test]
fn quotes_normalize() {
    let content = "'aaa' \"bbb\" \\'ccc\\' \\\"ddd\\\"";
    assert_eq!(
        oxtree::format::quotes_normalize(content, "\""),
        "'aaa' \\\"bbb\\\" \\'ccc\\' \\\"ddd\\\""
    );
}
