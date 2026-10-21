//! Port of the parts of `lib/rubocop/lockfile.rb` and `Config`
//! (`gem_versions_in_target`, `path_sourced_gems_in_target`) that Lint/NameTypo uses: the names of the
//! target's locked gems, and which of them come from a local path; and
//! their versions, for `Cop::Base#requires_gem` / `target_gem_version`.
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
    /// `gem_versions_in_target`: each locked gem's version (the last spec
    /// of that name, as `specs.to_h` keeps).
    pub versions: Vec<(String, String)>,
}

impl TargetGems {
    /// `gem_versions_in_target[gem_name]`.
    pub fn version(&self, gem_name: &str) -> Option<crate::gem_version::Version> {
        let (_, version) = self.versions.iter().find(|(name, _)| name == gem_name)?;
        crate::gem_version::Version::new(version)
    }
}

/// `Base#target_satisfies_all_gem_version_requirements?`: every
/// `requires_gem` requirement is met by the target's locked version (a
/// target without a lockfile, or without the gem, satisfies none).
pub fn satisfies_all_gem_version_requirements(
    config: &Config,
    gem_requirements: &[(&str, &[&str])],
) -> bool {
    if gem_requirements.is_empty() {
        return true;
    }
    let Some(gems) = target_gems(config) else {
        return false;
    };
    gem_requirements.iter().all(|(gem_name, requirements)| {
        let Some(version) = gems.version(gem_name) else {
            return false;
        };
        crate::gem_version::Requirement::new(requirements)
            .is_some_and(|requirement| requirement.is_satisfied_by(&version))
    })
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
        // Bundler's `NAME_VERSION`: `name (version[-platform])`.
        if let Some(version) = spec[name.len()..]
            .strip_prefix(" (")
            .and_then(|rest| rest.split(')').next())
            .map(|version| version.split('-').next().unwrap_or(version))
        {
            match gems.versions.iter_mut().find(|(n, _)| n == name) {
                Some((_, existing)) => *existing = version.to_owned(),
                None => gems.versions.push((name.to_owned(), version.to_owned())),
            }
        }
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
        assert_eq!(gems.version("nokogiri").unwrap().as_str(), "1.16.0");
        assert_eq!(gems.version("rails").unwrap().as_str(), "7.1.0");
        assert!(gems.version("activerecord").is_none());
    }

    // `Base#target_satisfies_all_gem_version_requirements?`
    #[test]
    fn gem_requirements_need_a_satisfying_locked_version() {
        let dir = std::env::temp_dir().join(format!("oxcop-requires-gem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Gemfile.lock"),
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    rails (7.1.3)\n\nBUNDLED WITH\n   2.5.0\n",
        )
        .unwrap();
        let config = Config::new(Default::default(), Some(dir.join(".rubocop.yml")));
        let satisfied =
            |reqs: &[(&str, &[&str])]| satisfies_all_gem_version_requirements(&config, reqs);
        assert!(satisfied(&[]));
        assert!(satisfied(&[("rails", &[">= 7.0"])]));
        assert!(satisfied(&[("rails", &[])]));
        assert!(!satisfied(&[("rails", &[">= 7.2"])]));
        assert!(!satisfied(&[("sidekiq", &[])]));
        let without_lockfile = Config::new(Default::default(), None);
        assert!(!satisfies_all_gem_version_requirements(
            &without_lockfile,
            &[("rails", &[])]
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
