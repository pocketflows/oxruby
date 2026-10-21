//! Port of `lib/rubocop/target_ruby.rb`: the target Ruby version comes from
//! the first of these sources that gives one: `RUBOCOP_TARGET_RUBY_VERSION`,
//! `AllCops: TargetRubyVersion`, a gemspec's `required_ruby_version`,
//! `.ruby-version`, `mise.toml`, `.tool-versions`, the `RUBY VERSION` of
//! `Gemfile.lock` / `gems.locked`, and the default.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::config::Config;
use crate::file_finder;
use crate::gem_version::{Requirement, Version};

pub const KNOWN_RUBIES: &[f64] = &[
    2.0, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 3.0, 3.1, 3.2, 3.3, 3.4, 4.0, 4.1,
];
pub const DEFAULT_VERSION: f64 = 2.7;

/// `TargetRuby::Source`: the version and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub version: f64,
    /// `Source#name`, e.g. "`.ruby-version`".
    pub name: String,
}

/// `TargetRuby#version`, cached per configuration (`Config#target_ruby_version`).
pub fn target_ruby_version(config: &Config) -> f64 {
    config.target_ruby().version
}

/// `TargetRuby#source`: the first source that finds a version.
pub fn source(config: &Config) -> Source {
    let found = rubocop_env_var()
        .or_else(|| rubocop_config(config))
        .or_else(|| gemspec_file(config))
        .or_else(|| version_file(config, ".ruby-version", &RUBY_VERSION_PATTERN))
        .or_else(|| version_file(config, "mise.toml", &MISE_TOML_PATTERN))
        .or_else(|| version_file(config, ".tool-versions", &TOOL_VERSIONS_PATTERN))
        .or_else(|| bundler_lock_file(config));
    found.unwrap_or_else(|| Source {
        version: DEFAULT_VERSION,
        name: "default".to_owned(),
    })
}

/// `TargetRuby.supported_versions` / `#supported?`.
pub fn is_supported(version: f64) -> bool {
    KNOWN_RUBIES.contains(&version)
}

/// Format a version the way Ruby prints a Float (`3.0`, `3.4`).
pub fn format_version(version: f64) -> String {
    format!("{version:?}")
}

/// `RuboCopEnvVar`: `ENV.fetch('RUBOCOP_TARGET_RUBY_VERSION', nil)&.to_f`.
fn rubocop_env_var() -> Option<Source> {
    env_var_source(std::env::var("RUBOCOP_TARGET_RUBY_VERSION").ok())
}

fn env_var_source(value: Option<String>) -> Option<Source> {
    let value = value?;
    Some(Source {
        version: ruby_to_f(&value),
        name: "`RUBOCOP_TARGET_RUBY_VERSION` environment variable".to_owned(),
    })
}

/// `RuboCopConfig`: `for_all_cops['TargetRubyVersion']&.to_f`.
fn rubocop_config(config: &Config) -> Option<Source> {
    let version = match config.for_all_cops().get("TargetRubyVersion")? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => ruby_to_f(s),
        _ => return None,
    };
    let path = config
        .loaded_path()
        .map(crate::path_util::smart_path)
        .unwrap_or_default();
    Some(Source {
        version,
        name: format!("`TargetRubyVersion` parameter (in {path})"),
    })
}

/// `GemspecFile`: the oldest known Ruby that satisfies the one gemspec's
/// `required_ruby_version`.
fn gemspec_file(config: &Config) -> Option<Source> {
    let file = gemspec_filepath(config)?;
    if !file.is_file() {
        return None;
    }
    let version = version_from_gemspec_file(&file)?;
    Some(Source {
        version,
        name: format!(
            "`required_ruby_version` parameter (in {})",
            file.to_string_lossy()
        ),
    })
}

/// `GemspecFile#gemspec_filepath`: the nearest directory, upwards from the
/// configuration's base directory, that holds exactly one `*.gemspec`.
fn gemspec_filepath(config: &Config) -> Option<PathBuf> {
    let mut found = None;
    file_finder::traverse_directories_upwards(config.base_dir(), None, |dir| {
        let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.starts_with('.') && name.ends_with(".gemspec"))
            })
            .collect();
        if candidates.len() == 1 {
            found = candidates.pop();
            return false;
        }
        true
    });
    found
}

static REQUIRED_RUBY_VERSION: NodePattern = NodePattern::new("(send _ :required_ruby_version= $_)");

static GEM_REQUIREMENT_VERSIONS: NodePattern = NodePattern::new(
    "(send (const (const _ :Gem) :Requirement) :new
       {$str+ | (send $str :freeze)+ | (array $str+) | (array (send $str :freeze)+)})",
);

/// `GemspecFile#version_from_gemspec_file`, then `find_minimal_known_ruby`.
fn version_from_gemspec_file(file: &Path) -> Option<f64> {
    let bytes = std::fs::read(file).ok()?;
    let raw = String::from_utf8(bytes).ok()?;
    let source = crate::processed_source::normalize_newlines(&raw);
    // RuboCop parses the gemspec at 3.3 with Prism.
    let processed_source = crate::processed_source::ProcessedSource::new(&raw, &source, 3.3, file);
    let ast = processed_source.ast()?;
    let (_, captures) = REQUIRED_RUBY_VERSION.search(ast).into_iter().next()?;
    let right_hand_side = captures.first()?.node()?;
    find_minimal_known_ruby(right_hand_side)
}

/// `GemspecFile#version_from_right_hand_side`.
fn version_from_right_hand_side(right_hand_side: Node<'_>) -> Option<Vec<String>> {
    let strings = |nodes: Vec<Node<'_>>| -> Vec<String> {
        nodes
            .into_iter()
            .filter_map(|node| node.str_content().map(str::to_owned))
            .collect()
    };
    if right_hand_side.is(NodeType::Array)
        && right_hand_side
            .child_nodes()
            .all(|child| child.is(NodeType::Str))
    {
        return Some(strings(right_hand_side.child_nodes().collect()));
    }
    if let Some(captures) = GEM_REQUIREMENT_VERSIONS.match_node(right_hand_side) {
        let capture = captures.first()?;
        let nodes = match capture.node() {
            Some(node) => vec![node],
            None => capture.nodes(),
        };
        return Some(strings(nodes));
    }
    if right_hand_side.is(NodeType::Str) {
        return Some(vec![right_hand_side.str_content()?.to_owned()]);
    }
    None
}

/// `GemspecFile#find_minimal_known_ruby`.
fn find_minimal_known_ruby(right_hand_side: Node<'_>) -> Option<f64> {
    let version = version_from_right_hand_side(right_hand_side)?;
    let requirement = Requirement::new(&version)?;
    KNOWN_RUBIES.iter().copied().find(|v| {
        Version::new(&format!("{}.99", format_version(*v)))
            .is_some_and(|v| requirement.is_satisfied_by(&v))
    })
}

static RUBY_VERSION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A(?:ruby-)?(?<version>\d+\.\d+)").unwrap());
static MISE_TOML_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^ruby = ["'](?<version>\d+\.\d+)"#).unwrap());
static TOOL_VERSIONS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(?:ruby )(?<version>\d+\.\d+)").unwrap());

/// `RubyVersionFile`, `MiseTomlFile` and `ToolVersionsFile`: the nearest
/// such file upwards from the configuration's base directory.
fn version_file(config: &Config, filename: &str, pattern: &Regex) -> Option<Source> {
    let file = file_finder::find_file_upwards(filename, config.base_dir(), None)?;
    let contents = std::fs::read_to_string(&file).ok()?;
    let version = pattern.captures(&contents)?.name("version")?.as_str();
    Some(Source {
        version: ruby_to_f(version),
        name: format!("`{filename}`"),
    })
}

/// `BundlerLockFile`: the `RUBY VERSION` section of the lockfile.
fn bundler_lock_file(config: &Config) -> Option<Source> {
    static RUBY_SECTION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*RUBY\s*VERSION\s*$").unwrap());
    static RUBY_LINE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*ruby\s+(\d+\.\d+)[p.\d]*\s*$").unwrap());

    let path = config.bundler_lock_file_path()?;
    let contents = std::fs::read(&path).ok()?;
    let contents = String::from_utf8_lossy(&contents);
    let mut in_ruby_section = false;
    for line in contents.split_inclusive('\n') {
        let line = line.strip_suffix('\n').unwrap_or(line);
        in_ruby_section = in_ruby_section || RUBY_SECTION.is_match(line);
        if !in_ruby_section {
            continue;
        }
        if let Some(captures) = RUBY_LINE.captures(line) {
            return Some(Source {
                version: ruby_to_f(&captures[1]),
                name: format!("`{}`", path.to_string_lossy()),
            });
        }
    }
    None
}

/// Ruby's `String#to_f`: the leading decimal number, else `0.0`.
fn ruby_to_f(s: &str) -> f64 {
    static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A\s*[+-]?(?:\d[\d_]*)(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?").unwrap()
    });
    FLOAT
        .find(s)
        .and_then(|m| m.as_str().trim().replace('_', "").parse().ok())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target Ruby version for a project holding `files`, whose
    /// `.rubocop.yml` holds `config`.
    fn version_in(name: &str, config: &str, files: &[(&str, &str)]) -> f64 {
        let dir =
            std::env::temp_dir().join(format!("oxcop-target-ruby-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (path, contents) in files {
            std::fs::write(dir.join(path), contents).unwrap();
        }
        let hash = serde_yaml_ng::from_str(if config.is_empty() { "{}" } else { config }).unwrap();
        let config = Config::new(hash, Some(dir.join(".rubocop.yml")));
        let version = source(&config).version;
        std::fs::remove_dir_all(&dir).unwrap();
        version
    }

    fn gemspec(required_ruby_version: &str) -> String {
        format!(
            "Gem::Specification.new do |s|\n  s.name = 'test'\n  s.required_ruby_version = \
             {required_ruby_version}\n  s.licenses = ['MIT']\nend\n"
        )
    }

    // target_ruby_spec.rb: "when gemspec file is present"
    #[test]
    fn the_gemspec_gives_the_oldest_known_ruby_it_allows() {
        for (rhs, expected) in [
            ("'>= 3.2.2'", 3.2),
            ("'> 3.2.2'", 3.2),
            ("'~> 3.2.0'", 3.2),
            ("Gem::Requirement.new('>= 2.3.1')", 2.3),
            ("'>= 1.9.0'", 2.0),
            ("Gem::Requirement.new('< 3.0.0')", 2.0),
            ("Gem::Requirement.new('>= 2.3.1', '< 3.0.0')", 2.3),
            ("Gem::Requirement.new(['>= 2.3.1', '< 3.0.0'])", 2.3),
            (
                "Gem::Requirement.new('>= 2.3.1'.freeze, '< 3.0.0'.freeze)",
                2.3,
            ),
            (
                "Gem::Requirement.new(['>= 2.3.1'.freeze, '< 3.0.0'.freeze])",
                2.3,
            ),
            ("['<=3.3.0', '>=3.1.3']", 3.1),
            ("['<=3.3.0', '>3.1.1', '~>3.2.1']", 3.2),
            (
                "Gem::Requirement.new(\">= #{File.read('.ruby-version').rstrip}\")",
                DEFAULT_VERSION,
            ),
            (
                "[\">= #{File.read('.ruby-version').rstrip}\"]",
                DEFAULT_VERSION,
            ),
            ("'>= 3.3.0'' # invalid syntax", DEFAULT_VERSION),
        ] {
            let spec = gemspec(rhs);
            assert_eq!(
                version_in("gemspec", "", &[("example.gemspec", &spec)]),
                expected,
                "{rhs}"
            );
        }
        // No `required_ruby_version`: the other sources are checked.
        let spec = "Gem::Specification.new do |s|\n  s.name = 'test'\nend\n";
        let files = [("example.gemspec", spec), (".ruby-version", "3.1.4\n")];
        assert_eq!(version_in("gemspec-none", "", &files), 3.1);
        // Two gemspecs: neither is used.
        let spec = gemspec("'~> 3.2.0'");
        let files = [("a.gemspec", spec.as_str()), ("b.gemspec", spec.as_str())];
        assert_eq!(version_in("gemspec-two", "", &files), DEFAULT_VERSION);
    }

    #[test]
    fn target_ruby_version_comes_first_then_the_files_in_order() {
        let config = "AllCops:\n  TargetRubyVersion: 2.5\n";
        let files = [
            (".ruby-version", "3.1.4\n"),
            ("Gemfile.lock", "RUBY VERSION\n   ruby 3.3.0p0\n"),
        ];
        assert_eq!(version_in("config", config, &files), 2.5);
        assert_eq!(env_var_source(Some("2.7".to_owned())).unwrap().version, 2.7);
        assert!(env_var_source(None).is_none());

        for (file, contents, expected) in [
            (".ruby-version", "2.4.10\n", 2.4),
            (".ruby-version", "3.10.1\n", 3.10),
            (".ruby-version", "ruby-3.4.1\n", 3.4),
            (".ruby-version", "jruby-9.1.2.0\n", DEFAULT_VERSION),
            (".ruby-version", "system\n", DEFAULT_VERSION),
            (".tool-versions", "nodejs 14.9.0\n", DEFAULT_VERSION),
            (".tool-versions", "nodejs 14.9.0\nruby 3.0.0\n", 3.0),
            ("mise.toml", "[tools]\nruby = \"3.4.2\"\n", 3.4),
            ("mise.toml", "[tools]\nnode = '20'\n", DEFAULT_VERSION),
            (
                "Gemfile.lock",
                "GEM\n  specs:\n\nRUBY VERSION\n   ruby 2.6.3p62\n\nBUNDLED WITH\n",
                2.6,
            ),
            (
                "Gemfile.lock",
                "GEM\n  specs:\n\nBUNDLED WITH\n   2.1.4\n",
                DEFAULT_VERSION,
            ),
            ("gems.locked", "RUBY VERSION\n   ruby 3.2.1\n", 3.2),
        ] {
            assert_eq!(
                version_in("file", "", &[(file, contents)]),
                expected,
                "{file}: {contents:?}"
            );
        }
        // .ruby-version wins over .tool-versions and the lockfile.
        let files = [
            (".ruby-version", "3.1.4\n"),
            (".tool-versions", "ruby 3.0.0\n"),
            ("Gemfile.lock", "RUBY VERSION\n   ruby 3.3.0p0\n"),
        ];
        assert_eq!(version_in("order", "", &files), 3.1);
    }

    #[test]
    fn ruby_to_f_reads_the_leading_number() {
        assert_eq!(ruby_to_f("3.4"), 3.4);
        assert_eq!(ruby_to_f(" 3.1.2"), 3.1);
        assert_eq!(ruby_to_f("3"), 3.0);
        assert_eq!(ruby_to_f("abc"), 0.0);
        assert_eq!(ruby_to_f(""), 0.0);
    }
}
