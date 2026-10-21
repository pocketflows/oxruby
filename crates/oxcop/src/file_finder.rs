//! Port of `lib/rubocop/file_finder.rb`. `FileFinder.root_level` is only
//! set by RuboCop's own specs, so it isn't ported.

use std::path::{Path, PathBuf};

use crate::path_util;

/// `FileFinder#find_file_upwards`: the nearest `filename` in `start_dir` or
/// above, stopping after `stop_dir`.
pub fn find_file_upwards(
    filename: &str,
    start_dir: &Path,
    stop_dir: Option<&Path>,
) -> Option<PathBuf> {
    let mut found = None;
    traverse_files_upwards(filename, start_dir, stop_dir, |file| {
        found = Some(file);
        false
    });
    found
}

/// `FileFinder#find_last_file_upwards`: the topmost `filename` from
/// `start_dir` up to `stop_dir`.
pub fn find_last_file_upwards(
    filename: &str,
    start_dir: &Path,
    stop_dir: Option<&Path>,
) -> Option<PathBuf> {
    let mut last = None;
    traverse_files_upwards(filename, start_dir, stop_dir, |file| {
        last = Some(file);
        true
    });
    last
}

/// `FileFinder#traverse_directories_upwards`: yields `start_dir` (expanded)
/// and each parent, stopping after `stop_dir`, or as soon as `f` returns
/// false (Ruby's `break`).
pub fn traverse_directories_upwards(
    start_dir: &Path,
    stop_dir: Option<&Path>,
    mut f: impl FnMut(&Path) -> bool,
) {
    let start = path_util::expand_path(start_dir);
    for dir in start.ancestors() {
        if !f(dir) {
            return;
        }
        if stop_dir.is_some_and(|stop| stop == dir) {
            return;
        }
    }
}

fn traverse_files_upwards(
    filename: &str,
    start_dir: &Path,
    stop_dir: Option<&Path>,
    mut f: impl FnMut(PathBuf) -> bool,
) {
    traverse_directories_upwards(start_dir, stop_dir, |dir| {
        let file = dir.join(filename);
        if file.exists() { f(file) } else { true }
    });
}
