//! Compiled-in plugins (`oxcop::plugin`): a plugin's cops join the registry,
//! its default configuration is integrated, its `require:` claims silence
//! the warning, and name collisions are errors. Plugins are installed once
//! per process, so these tests live in their own test binary.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use oxcop::cli::{Io, Sink};
use oxcop::plugin::Plugin;
use oxcop::prelude::*;

/// Flags `forbidden` calls with the configured `Message`.
#[derive(Default)]
struct Forbidden;

impl Cop for Forbidden {
    fn name(&self) -> &'static str {
        "Test/Forbidden"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["forbidden"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let message = cx.cop_config_str("Message").unwrap_or("No message.");
        cx.add_offense(node.source_range(), message);
    }
}

/// A plugin cop in a built-in department, with autocorrection.
#[derive(Default)]
struct NoBang;

impl Cop for NoBang {
    fn name(&self) -> &'static str {
        "Style/TestNoBang"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["save!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if regex!(r"\Asave!\z").is_match(node.method_name().unwrap_or_default()) {
            let selector = node.loc().selector.unwrap();
            cx.add_offense_with_correction(selector, "Use `save`.", |corrector| {
                corrector.replace(selector, "save");
            });
        }
    }
}

const DEFAULT_CONFIG: &str = "\
Test/Forbidden:
  Description: Flags forbidden calls.
  Message: The default message.
Style/TestNoBang:
  Description: Flags save!.
  SafeAutoCorrect: false
";

fn plugin() -> Plugin {
    Plugin::new("test-cops")
        .default_config(DEFAULT_CONFIG)
        .provides_require("./lib/rubocop/cop/test/forbidden")
        .ignores_require("./lib/rubocop/exit_hook")
        .cop::<Forbidden>()
        .cop::<NoBang>()
}

/// Installs the plugin once, and serializes the runs (they set the working
/// directory).
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static INSTALLED: LazyLock<()> = LazyLock::new(|| {
        oxcop::Oxcop::new().plugin(plugin()).install().unwrap();
    });
    static LOCK: Mutex<()> = Mutex::new(());
    LazyLock::force(&INSTALLED);
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Run {
    status: i32,
    stdout: String,
    stderr: String,
}

/// Runs oxcop in `dir` (as the working directory) on `files`.
fn oxcop(dir: &Path, args: &[&str]) -> Run {
    oxcop::path_util::set_pwd_override(Some(dir.to_path_buf()));
    let mut io = Io {
        stdin: Box::new(std::io::empty()),
        stdout: Sink::buffer(),
        stderr: Sink::buffer(),
    };
    let args = std::iter::once("oxcop")
        .chain(["--cache", "false"])
        .chain(args.iter().copied())
        .map(Into::into);
    let status = oxcop::cli::run_with(args, &mut io);
    oxcop::path_util::set_pwd_override(None);
    Run {
        status,
        stdout: String::from_utf8(io.stdout.take()).unwrap(),
        stderr: String::from_utf8(io.stderr.take()).unwrap(),
    }
}

fn fixture_dir(name: &str, config: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("plugin")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".rubocop.yml"), config).unwrap();
    for (file, source) in files {
        std::fs::write(dir.join(file), source).unwrap();
    }
    dir
}

const CONFIG: &str = "\
require:
  - ./lib/rubocop/cop/test/forbidden
  - ./lib/rubocop/exit_hook
AllCops:
  DisabledByDefault: true
  SuggestExtensions: false
Test/Forbidden:
  Enabled: true
Style/TestNoBang:
  Enabled: true
";

#[test]
fn plugin_cops_join_the_registry() {
    let _lock = lock();
    let names = oxcop::cop::registry::names();
    assert!(names.contains(&"Test/Forbidden"));
    assert!(names.contains(&"Style/TestNoBang"));
    // Still sorted by name.
    assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
    let dir = fixture_dir("show", CONFIG, &[]);
    let run = oxcop(&dir, &["--show-ported-cops"]);
    assert!(run.stdout.lines().any(|line| line == "Test/Forbidden"));
}

#[test]
fn plugin_cops_run_with_their_default_configuration() {
    let _lock = lock();
    let dir = fixture_dir(
        "default_config",
        CONFIG,
        &[("a.rb", "forbidden(1)\nrecord.save!\n")],
    );
    let run = oxcop(&dir, &["--format", "emacs", "a.rb"]);
    assert_eq!(run.status, 1, "{}", run.stderr);
    let file = dir.join("a.rb");
    assert_eq!(
        run.stdout,
        format!(
            "{0}:1:1: C: Test/Forbidden: The default message.\n\
             {0}:2:8: C: [Correctable] Style/TestNoBang: Use `save`.\n",
            file.display()
        )
    );
    // The claimed `require:` entries don't warn.
    assert_eq!(run.stderr, "");
}

#[test]
fn project_configuration_overrides_the_plugin_default() {
    let _lock = lock();
    let config = CONFIG.replace(
        "Test/Forbidden:\n  Enabled: true\n",
        "Test/Forbidden:\n  Enabled: true\n  Message: Overridden.\n",
    );
    let dir = fixture_dir("override", &config, &[("a.rb", "forbidden\n")]);
    let run = oxcop(&dir, &["--format", "emacs", "a.rb"]);
    assert!(
        run.stdout.contains("Test/Forbidden: Overridden."),
        "{}",
        run.stdout
    );
}

#[test]
fn unclaimed_requires_still_warn() {
    let _lock = lock();
    let config = CONFIG.replace(
        "  - ./lib/rubocop/exit_hook\n",
        "  - ./lib/rubocop/exit_hook\n  - ./lib/rubocop/cop/test/other\n",
    );
    let dir = fixture_dir("unclaimed", &config, &[("a.rb", "x = 1\nx\n")]);
    let run = oxcop(&dir, &["--format", "emacs", "a.rb"]);
    assert_eq!(
        run.stderr,
        "oxcop: Ruby extensions in `require` are not supported and will be ignored: \
         ./lib/rubocop/cop/test/other\n"
    );
}

#[test]
fn directives_know_plugin_cops_and_departments() {
    let _lock = lock();
    let config = CONFIG.replace(
        "AllCops:\n",
        "Lint/RedundantCopDisableDirective:\n  Enabled: true\nAllCops:\n",
    );
    let source = "\
forbidden # rubocop:disable Test
forbidden # rubocop:disable Test/Forbidden
x = 1 # rubocop:disable Test/Forbidden
";
    let dir = fixture_dir("directives", &config, &[("a.rb", source)]);
    let run = oxcop(&dir, &["--format", "emacs", "a.rb"]);
    let file = dir.join("a.rb");
    assert_eq!(
        run.stdout,
        format!(
            "{}:3:7: W: [Correctable] Lint/RedundantCopDisableDirective: \
             Unnecessary disabling of `Test/Forbidden`.\n",
            file.display()
        )
    );

    let run = oxcop(
        &dir,
        &["--format", "emacs", "--ignore-disable-comments", "a.rb"],
    );
    assert_eq!(run.stdout.matches("Test/Forbidden: The default").count(), 2);
}

#[test]
fn only_selects_plugin_cops() {
    let _lock = lock();
    let dir = fixture_dir("only", CONFIG, &[("a.rb", "forbidden(1)\nrecord.save!\n")]);
    let run = oxcop(
        &dir,
        &["--format", "emacs", "--only", "Style/TestNoBang", "a.rb"],
    );
    assert_eq!(run.stdout.lines().count(), 1, "{}", run.stdout);
    assert!(run.stdout.contains("Style/TestNoBang"));
}

#[test]
fn offenses_formatter_labels_plugin_cops() {
    let _lock = lock();
    let dir = fixture_dir(
        "offenses",
        CONFIG,
        &[("a.rb", "forbidden(1)\nrecord.save!\nrecord.save!\n")],
    );
    let run = oxcop(&dir, &["--format", "offenses", "a.rb"]);
    assert_eq!(
        run.stdout,
        "\n2  Style/TestNoBang [Unsafe Correctable]\n1  Test/Forbidden\n--\n3  Total in 1 files\n\n"
    );
}

#[test]
fn collisions_are_errors() {
    let _lock = lock();
    #[derive(Default)]
    struct HashSyntax;
    impl Cop for HashSyntax {
        fn name(&self) -> &'static str {
            "Style/HashSyntax"
        }
    }
    let error = oxcop::plugin::check(vec![Plugin::new("a").cop::<HashSyntax>()])
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "cop `Style/HashSyntax` of plugin `a` has the name of a built-in cop"
    );

    let error = oxcop::plugin::check(vec![
        Plugin::new("a").cop::<Forbidden>(),
        Plugin::new("b").cop::<Forbidden>(),
    ])
    .err()
    .unwrap();
    assert_eq!(
        error.to_string(),
        "cop `Test/Forbidden` of plugin `b` is already registered by plugin `a`"
    );

    let error = oxcop::plugin::check(vec![Plugin::new("a").default_config("- not a mapping")])
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "default configuration of plugin `a`: not a YAML mapping"
    );

    // Installing again (the registry is in use) fails too.
    let error = oxcop::Oxcop::new().plugin(plugin()).install().unwrap_err();
    assert_eq!(
        error.to_string(),
        "plugins must be installed once, before the cop registry is first used"
    );
}
