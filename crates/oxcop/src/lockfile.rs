//! Port of the parts of `lib/rubocop/lockfile.rb` and `Config`
//! (`gem_versions_in_target`, `path_sourced_gems_in_target`) that Lint/NameTypo uses: the names of the
//! target's locked gems, and which of them come from a local path.
//!
//! Bundler's `LockfileParser` is replaced by a reading of the lockfile's
//! sections: every `GEM`, `GIT` and `PATH` section lists its gems under
//! `specs:`, four spaces in (`    name (version)`); a `PATH` section's gems
//! are sourced from a local path.

use std::sync::{Arc, RwLock};

use crate::config::Config;

/// The gems of the target's lockfile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetGems {
    /// `gem_versions_in_target.keys`: every locked gem.
    pub names: Vec<String>,
    /// `path_sourced_gems_in_target`.
    pub path_sourced: Vec<String>,
}

/// What RuboCop's specs stub `gem_versions_in_target` and
/// `path_sourced_gems_in_target` with (`--target-gems`), for the current run.
static OVERRIDE: RwLock<Option<Arc<TargetGems>>> = RwLock::new(None);

pub fn set_target_gems_override(gems: Option<TargetGems>) {
    *OVERRIDE.write().unwrap_or_else(|e| e.into_inner()) = gems.map(Arc::new);
}

/// The target's gems as `config` sees them: `None` without a lockfile (as
/// `gem_versions_in_target` is `nil`).
pub fn target_gems(config: &Config) -> Option<Arc<TargetGems>> {
    if let Some(gems) = OVERRIDE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return Some(gems.clone());
    }
    let path = config.bundler_lock_file_path()?;
    let source = std::fs::read_to_string(path).ok()?;
    Some(Arc::new(parse(&source)))
}

/// Reads the gems of a lockfile.
pub fn parse(source: &str) -> TargetGems {
    let mut gems = TargetGems::default();
    let mut section = "";
    let mut in_specs = false;
    for line in source.lines() {
        if !line.starts_with(' ') {
            section = line.trim_end();
            in_specs = false;
            continue;
        }
        if !matches!(section, "GEM" | "GIT" | "PATH") {
            continue;
        }
        if line.trim_end() == "  specs:" {
            in_specs = true;
            continue;
        }
        if line
            .strip_prefix("  ")
            .is_some_and(|rest| !rest.starts_with(' '))
        {
            // `remote:`, `revision:` and the like.
            in_specs = false;
            continue;
        }
        // `    name (version)`; deeper lines are the gem's dependencies.
        let Some(spec) = line
            .strip_prefix("    ")
            .filter(|spec| !spec.starts_with(' '))
        else {
            continue;
        };
        if !in_specs {
            continue;
        }
        let Some(name) = spec.split(' ').next().filter(|name| !name.is_empty()) else {
            continue;
        };
        if !gems.names.iter().any(|n| n == name) {
            gems.names.push(name.to_owned());
        }
        if section == "PATH" && !gems.path_sourced.iter().any(|n| n == name) {
            gems.path_sourced.push(name.to_owned());
        }
    }
    gems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gems_and_path_sources() {
        let gems = parse(
            "PATH\n  remote: .\n  specs:\n    my_app (0.1.0)\n      rails (>= 7)\n\n\
             GIT\n  remote: https://example.com/x.git\n  revision: abc\n  specs:\n    flipper (1.0)\n\n\
             GEM\n  remote: https://rubygems.org/\n  specs:\n    rails (7.1.0)\n      railties (= 7.1.0)\n    \
             railties (7.1.0)\n    nokogiri (1.16.0-arm64-darwin)\n\n\
             PLATFORMS\n  arm64-darwin\n\nDEPENDENCIES\n  rails\n\nBUNDLED WITH\n   2.5.0\n",
        );
        assert_eq!(
            gems.names,
            ["my_app", "flipper", "rails", "railties", "nokogiri"]
        );
        assert_eq!(gems.path_sourced, ["my_app"]);
    }
}
