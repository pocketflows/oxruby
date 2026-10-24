//! Port of `lib/rubocop/target_finder.rb`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_yaml_ng::Value;
use walkdir::WalkDir;

use crate::config::Config;
use crate::config_loader::{ConfigError, ConfigStore};
use crate::path_util::{self, MatchPath};

pub struct TargetFinder<'a> {
    config_store: &'a mut ConfigStore,
    force_exclusion: bool,
}

impl<'a> TargetFinder<'a> {
    pub fn new(config_store: &'a mut ConfigStore, force_exclusion: bool) -> Self {
        Self {
            config_store,
            force_exclusion,
        }
    }

    /// Files to inspect for the given command-line paths (the working
    /// directory when none are given), as absolute, sorted paths.
    pub fn find(&mut self, args: &[PathBuf]) -> Result<Vec<PathBuf>, ConfigError> {
        let args = if args.is_empty() {
            vec![path_util::pwd()]
        } else {
            args.to_vec()
        };
        let mut files = Vec::new();
        for arg in args {
            let path = path_util::expand_path(&arg);
            if path.is_dir() {
                files.extend(self.target_files_in_dir(&path)?);
            } else if path.is_file() {
                if !self.force_exclusion
                    || !self.config_store.for_file(&path)?.file_to_exclude(&path)
                {
                    files.push(path);
                }
            } else {
                return Err(ConfigError(format!(
                    "No such file or directory: {}",
                    arg.display()
                )));
            }
        }
        files.sort();
        files.dedup();
        Ok(files)
    }

    fn target_files_in_dir(&mut self, base_dir: &Path) -> Result<Vec<PathBuf>, ConfigError> {
        let base_dir_config = self.config_store.for_dir(base_dir)?;
        let mut files = Vec::new();
        let walker = WalkDir::new(base_dir)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_type().is_dir()
                    || !excluded_dir(&base_dir_config, entry.path())
            });
        for entry in walker.filter_map(Result::ok) {
            if entry.file_type().is_file() && to_inspect(&base_dir_config, entry.path(), base_dir) {
                files.push(entry.into_path());
            }
        }
        Ok(files)
    }
}

/// Skip walking directories whose every file would be excluded (e.g. the
/// default `node_modules/**/*`).
fn excluded_dir(config: &Config, dir: &Path) -> bool {
    let probe = dir.join("__oxcop_probe__").to_string_lossy().into_owned();
    let probe = MatchPath::new(&probe);
    config
        .all_cops_exclude()
        .iter()
        .any(|pattern| pattern.as_str().ends_with("/**/*") && pattern.matches(&probe))
}

fn to_inspect(config: &Config, file: &Path, base_dir: &Path) -> bool {
    if config.file_to_exclude(file) {
        return false;
    }
    let hidden = file
        .strip_prefix(base_dir)
        .map(|relative| {
            relative
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false);
    if !hidden && ruby_executable(config, file) {
        return true;
    }
    config.file_to_include(file)
}

/// Extensionless files with a Ruby shebang (`RubyInterpreters`).
fn ruby_executable(config: &Config, file: &Path) -> bool {
    if file.extension().is_some() {
        return false;
    }
    let Ok(handle) = std::fs::File::open(file) else {
        return false;
    };
    let mut first_line = String::new();
    if BufReader::new(handle).read_line(&mut first_line).is_err() || !first_line.starts_with("#!") {
        return false;
    }
    let interpreters = config
        .for_all_cops()
        .get("RubyInterpreters")
        .and_then(Value::as_sequence);
    interpreters
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|interpreter| {
            first_line
                .split(|c: char| c.is_whitespace() || c == '/')
                .any(|word| word == interpreter)
        })
}
