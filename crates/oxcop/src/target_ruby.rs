//! Port of `lib/rubocop/target_ruby.rb` (config and `.ruby-version` sources
//! only so far).

use std::path::Path;

use crate::config::Config;

pub const DEFAULT_VERSION: f64 = 2.7;

pub fn target_ruby_version(config: &Config) -> f64 {
    if let Some(version) = config
        .for_all_cops()
        .get("TargetRubyVersion")
        // `TargetRubyVersion.to_f`: a string such as `'3.0'` works too.
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        })
    {
        return version;
    }
    from_ruby_version_file(config.base_dir()).unwrap_or(DEFAULT_VERSION)
}

/// Format a version the way Ruby prints a Float (`3.0`, `3.4`).
pub fn format_version(version: f64) -> String {
    format!("{version:?}")
}

fn from_ruby_version_file(dir: &Path) -> Option<f64> {
    let contents = std::fs::read_to_string(dir.join(".ruby-version")).ok()?;
    let version = contents.trim().trim_start_matches("ruby-");
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    format!("{major}.{minor}").parse().ok()
}
