//! Port of `lib/rubocop/path_util.rb`, plus a port of Ruby's `File.fnmatch`
//! (from `dir.c`) so that `Include`/`Exclude` patterns match exactly as they
//! do in RuboCop.

use std::path::{Component, Path, PathBuf};

/// `Dir.pwd` when it isn't the process's working directory: `oxcop
/// --server` answers for a spec that stubs `Dir.pwd` with a directory that
/// doesn't exist (see `server.rs`).
static PWD_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub fn set_pwd_override(dir: Option<PathBuf>) {
    *PWD_OVERRIDE.write().unwrap_or_else(|e| e.into_inner()) = dir;
}

pub fn pwd() -> PathBuf {
    if let Some(dir) = PWD_OVERRIDE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        return dir.clone();
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Paths under the working directory are shown relative to it; anything else
/// is shown as given.
pub fn smart_path(path: &Path) -> String {
    match path.strip_prefix(pwd()) {
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// Like `File.expand_path`: absolute, with `.` and `..` resolved lexically.
pub fn expand_path(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        pwd().join(path)
    };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

pub fn relative_path(path: &Path, base_dir: &Path) -> String {
    match path.strip_prefix(base_dir) {
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

pub fn absolute(path: &str) -> bool {
    path.starts_with('/')
}

pub fn glob(pattern: &str) -> bool {
    pattern.contains(['*', '?', '[', '{'])
}

/// Ruby's `Dir.glob(pattern)` (no flags): braces expanded, `*`, `?` and
/// `[...]` within a path segment, `**/` for any number of directories;
/// wildcards don't match a leading `.`. Relative patterns are relative to
/// the working directory and so are the results. Each directory is read in
/// sorted order, as `Dir.glob`'s `sort: true` does.
pub fn dir_glob(pattern: &str) -> Vec<String> {
    let mut found = Vec::new();
    for expanded in brace_expand(pattern, 0) {
        let (base, prefix) = if expanded.starts_with('/') {
            (PathBuf::from("/"), "/".to_owned())
        } else {
            (pwd(), String::new())
        };
        let segments: Vec<&str> = expanded.split('/').filter(|s| !s.is_empty()).collect();
        glob_segments(&base, &prefix, &segments, &mut found);
    }
    found
}

fn glob_segments(dir: &Path, prefix: &str, segments: &[&str], found: &mut Vec<String>) {
    let Some((segment, rest)) = segments.split_first() else {
        return;
    };
    let mut descend = |name: &str, segments: &[&str]| {
        let path = dir.join(name);
        if segments.is_empty() {
            if path.exists() {
                found.push(format!("{prefix}{name}"));
            }
        } else if path.is_dir() {
            glob_segments(&path, &format!("{prefix}{name}/"), segments, found);
        }
    };
    if !glob(segment) {
        descend(segment, rest);
        return;
    }
    let mut entries: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    entries.sort();
    if *segment == "**" && !rest.is_empty() {
        // `**/`: no directory, then each directory below.
        glob_segments(dir, prefix, rest, found);
        for name in &entries {
            if dir.join(name).is_dir() {
                glob_segments(
                    &dir.join(name),
                    &format!("{prefix}{name}/"),
                    segments,
                    found,
                );
            }
        }
        return;
    }
    for name in &entries {
        if fnmatch(segment, name, FNM_PATHNAME) {
            descend(name, rest);
        }
    }
}

pub fn match_path(pattern: &str, path: &str) -> bool {
    let matches =
        pattern == path || (glob(pattern) && fnmatch(pattern, path, FNM_PATHNAME | FNM_EXTGLOB));
    matches || hidden_file_in_not_hidden_dir(pattern, path)
}

/// A `match_path` pattern brace-expanded once, for matching it against
/// many files (`AllCops` `Include`/`Exclude` during file discovery).
#[derive(Debug, Clone)]
pub struct PathPattern {
    source: String,
    glob: bool,
    expanded: Vec<Vec<char>>,
}

/// A path as `PathPattern::matches` takes it: its characters split out once.
pub struct MatchPath<'a> {
    text: &'a str,
    chars: Vec<char>,
    hidden_file_in_not_hidden_dir: bool,
}

impl<'a> MatchPath<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            chars: text.chars().collect(),
            hidden_file_in_not_hidden_dir: hidden_file(text) && !hidden_dir(text),
        }
    }
}

impl PathPattern {
    pub fn new(source: &str) -> Self {
        // `fnmatch` expands braces the same way with or without
        // `FNM_DOTMATCH`, so one expansion serves both `match_path` checks.
        let expanded = brace_expand(source, FNM_PATHNAME | FNM_EXTGLOB)
            .iter()
            .map(|pattern| pattern.chars().collect())
            .collect();
        Self {
            source: source.to_owned(),
            glob: glob(source),
            expanded,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// `match_path(pattern, path)`.
    pub fn matches(&self, path: &MatchPath) -> bool {
        let fnmatch = |flags| {
            self.expanded
                .iter()
                .any(|pattern| fnmatch_chars(pattern, &path.chars, flags))
        };
        self.source == path.text
            || (self.glob && fnmatch(FNM_PATHNAME | FNM_EXTGLOB))
            || (path.hidden_file_in_not_hidden_dir
                && fnmatch(FNM_PATHNAME | FNM_EXTGLOB | FNM_DOTMATCH))
    }
}

fn hidden_file_in_not_hidden_dir(pattern: &str, path: &str) -> bool {
    hidden_file(path)
        && fnmatch(pattern, path, FNM_PATHNAME | FNM_EXTGLOB | FNM_DOTMATCH)
        && !hidden_dir(path)
}

fn maybe_hidden_file(path: &str) -> bool {
    path.starts_with('.') || path.contains("/.")
}

fn hidden_file(path: &str) -> bool {
    maybe_hidden_file(path)
        && path
            .rsplit('/')
            .next()
            .is_some_and(|base| base.starts_with('.'))
}

fn hidden_dir(path: &str) -> bool {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').any(|part| part.starts_with('.')),
        None => false,
    }
}

pub const FNM_NOESCAPE: u32 = 0x01;
pub const FNM_PATHNAME: u32 = 0x02;
pub const FNM_DOTMATCH: u32 = 0x04;
pub const FNM_EXTGLOB: u32 = 0x10;

/// Port of Ruby's `File.fnmatch?` (case-sensitive).
pub fn fnmatch(pattern: &str, path: &str, flags: u32) -> bool {
    let string: Vec<char> = path.chars().collect();
    if flags & FNM_EXTGLOB != 0 {
        brace_expand(pattern, flags)
            .iter()
            .any(|expanded| fnmatch_chars(&expanded.chars().collect::<Vec<_>>(), &string, flags))
    } else {
        fnmatch_chars(&pattern.chars().collect::<Vec<_>>(), &string, flags)
    }
}

/// Port of `ruby_brace_expand`: `a{b,c{d,e}}` => `ab`, `acd`, `ace`.
pub fn brace_expand(pattern: &str, flags: u32) -> Vec<String> {
    let escape = flags & FNM_NOESCAPE == 0;
    let chars: Vec<char> = pattern.chars().collect();
    let mut nest = 0;
    let mut lbrace = None;
    let mut rbrace = None;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                if nest == 0 {
                    lbrace = Some(i);
                }
                nest += 1;
            }
            '}' => {
                nest -= 1;
                if nest == 0 && lbrace.is_some() {
                    rbrace = Some(i);
                    break;
                }
            }
            '\\' if escape => i += 1,
            _ => {}
        }
        i += 1;
    }

    let (Some(lbrace), Some(rbrace)) = (lbrace, rbrace) else {
        return vec![pattern.to_owned()];
    };
    let prefix: String = chars[..lbrace].iter().collect();
    let suffix: String = chars[rbrace + 1..].iter().collect();
    let mut results = Vec::new();
    let mut start = lbrace + 1;
    let mut nest = 0;
    let mut i = start;
    while i <= rbrace {
        match chars[i] {
            '{' => nest += 1,
            '}' if nest > 0 => nest -= 1,
            '\\' if escape && i + 1 < rbrace => i += 1,
            ',' | '}' if nest == 0 => {
                let alternative: String = chars[start..i].iter().collect();
                results.extend(brace_expand(
                    &format!("{prefix}{alternative}{suffix}"),
                    flags,
                ));
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    results
}

fn fnmatch_chars(p: &[char], s: &[char], flags: u32) -> bool {
    let period = flags & FNM_DOTMATCH == 0;
    if flags & FNM_PATHNAME == 0 {
        let (mut pi, mut si) = (0, 0);
        return fnmatch_helper(p, &mut pi, s, &mut si, flags);
    }

    let (mut pi, mut si) = (0, 0);
    let mut ptmp: Option<usize> = None;
    let mut stmp: Option<usize> = None;
    let at = |v: &[char], i: usize| v.get(i).copied();
    loop {
        if at(p, pi) == Some('*') && at(p, pi + 1) == Some('*') && at(p, pi + 2) == Some('/') {
            while at(p, pi) == Some('*') && at(p, pi + 1) == Some('*') && at(p, pi + 2) == Some('/')
            {
                pi += 3;
            }
            ptmp = Some(pi);
            stmp = Some(si);
        }
        if fnmatch_helper(p, &mut pi, s, &mut si, flags) {
            while si < s.len() && s[si] != '/' {
                si += 1;
            }
            if pi < p.len() && si < s.len() {
                pi += 1;
                si += 1;
                continue;
            }
            if pi >= p.len() && si >= s.len() {
                return true;
            }
        }
        // Failed: try the next recursion of the last `**/`.
        if let (Some(pt), Some(mut st)) = (ptmp, stmp)
            && !(period && at(s, st) == Some('.'))
        {
            while st < s.len() && s[st] != '/' {
                st += 1;
            }
            if st < s.len() {
                pi = pt;
                st += 1;
                si = st;
                stmp = Some(st);
                continue;
            }
        }
        return false;
    }
}

/// Matches a single path segment (when `FNM_PATHNAME`), advancing `pcur` and
/// `scur` to where matching stopped.
fn fnmatch_helper(p: &[char], pcur: &mut usize, s: &[char], scur: &mut usize, flags: u32) -> bool {
    let period = flags & FNM_DOTMATCH == 0;
    let pathname = flags & FNM_PATHNAME != 0;
    let escape = flags & FNM_NOESCAPE == 0;
    let is_end = |v: &[char], i: usize| i >= v.len() || (pathname && v[i] == '/');
    let unescape = |i: usize| {
        if escape && p.get(i) == Some(&'\\') {
            i + 1
        } else {
            i
        }
    };

    let mut pi = *pcur;
    let mut si = *scur;
    let mut ptmp: Option<usize> = None;
    let mut stmp: Option<usize> = None;

    macro_rules! ret {
        ($val:expr) => {{
            *pcur = pi;
            *scur = si;
            return $val;
        }};
    }

    if period && s.get(si) == Some(&'.') && p.get(unescape(pi)) != Some(&'.') {
        ret!(false);
    }

    loop {
        let mut failed = false;
        match p.get(pi) {
            Some('*') => {
                while p.get(pi) == Some(&'*') {
                    pi += 1;
                }
                if is_end(p, unescape(pi)) {
                    pi = unescape(pi);
                    ret!(true);
                }
                if is_end(s, si) {
                    ret!(false);
                }
                ptmp = Some(pi);
                stmp = Some(si);
                continue;
            }
            Some('?') => {
                if is_end(s, si) {
                    ret!(false);
                }
                pi += 1;
                si += 1;
                continue;
            }
            Some('[') => {
                if is_end(s, si) {
                    ret!(false);
                }
                match bracket(p, pi + 1, s[si], flags) {
                    Some(t) => {
                        pi = t;
                        si += 1;
                        continue;
                    }
                    None => failed = true,
                }
            }
            _ => {}
        }

        if !failed {
            pi = unescape(pi);
            if is_end(s, si) {
                ret!(is_end(p, pi));
            }
            if is_end(p, pi) {
                failed = true;
            } else if p[pi] == s[si] {
                pi += 1;
                si += 1;
                continue;
            } else {
                failed = true;
            }
        }

        if failed {
            if let (Some(pt), Some(st)) = (ptmp, stmp) {
                pi = pt;
                stmp = Some(st + 1);
                si = st + 1;
                continue;
            }
            ret!(false);
        }
    }
}

/// Port of `bracket` from `dir.c`. Returns the index just past the closing
/// `]` when `c` matches the character class starting at `pi`.
fn bracket(p: &[char], mut pi: usize, c: char, flags: u32) -> Option<usize> {
    let escape = flags & FNM_NOESCAPE == 0;
    let mut ok = false;
    let mut not = false;
    if pi >= p.len() {
        return None;
    }
    if p[pi] == '!' || p[pi] == '^' {
        not = true;
        pi += 1;
    }
    while p.get(pi) != Some(&']') {
        let mut t1 = pi;
        if escape && p.get(t1) == Some(&'\\') {
            t1 += 1;
        }
        let c1 = *p.get(t1)?;
        pi = t1 + 1;
        if pi >= p.len() {
            return None;
        }
        if p[pi] == '-' && p.get(pi + 1) != Some(&']') {
            let mut t2 = pi + 1;
            if escape && p.get(t2) == Some(&'\\') {
                t2 += 1;
            }
            let c2 = *p.get(t2)?;
            pi = t2 + 1;
            if ok {
                continue;
            }
            if c1 <= c && c <= c2 {
                ok = true;
            }
        } else if !ok && c1 == c {
            ok = true;
        }
    }
    if ok == not { None } else { Some(pi + 1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAGS: u32 = FNM_PATHNAME | FNM_EXTGLOB;

    #[test]
    fn matches_like_ruby_fnmatch() {
        assert!(fnmatch("**/*.rb", "a.rb", FLAGS));
        assert!(fnmatch("**/*.rb", "lib/a/b.rb", FLAGS));
        assert!(!fnmatch("**/*.rb", "lib/.hidden/b.rb", FLAGS));
        assert!(!fnmatch("**/*.rb", ".a.rb", FLAGS));
        assert!(fnmatch("**/*.rb", ".a.rb", FLAGS | FNM_DOTMATCH));
        assert!(fnmatch(
            "/x/node_modules/**/*",
            "/x/node_modules/a/b.js",
            FLAGS
        ));
        assert!(!fnmatch("tmp/*.rb", "tmp/a/b.rb", FLAGS));
        assert!(fnmatch("**/{Gemfile,gems.rb}", "app/gems.rb", FLAGS));
        assert!(fnmatch("**/[Rr]akefile", "Rakefile", FLAGS));
        assert!(!fnmatch("**/[!R]akefile", "Rakefile", FLAGS));
        assert!(fnmatch("**/Gemfile", "Gemfile", FLAGS));
        assert!(fnmatch("*", "a/b", FNM_EXTGLOB));
    }

    #[test]
    fn match_path_handles_hidden_files_in_visible_dirs() {
        assert!(match_path("**/*", "lib/.rubocop.yml"));
        assert!(!match_path("**/*", ".git/config"));
    }

    #[test]
    fn path_pattern_matches_as_match_path() {
        let patterns = [
            "**/*.rb",
            "**/*",
            "**/{Gemfile,gems.rb}",
            "**/[Rr]akefile",
            "**/.pryrc",
            "tmp/**/*",
            "/x/node_modules/**/*",
            "db/*schema.rb",
            "Gemfile",
            "lib/{a,b{c,d}}/*.rb",
        ];
        let paths = [
            "a.rb",
            "lib/a/b.rb",
            "lib/.hidden/b.rb",
            ".a.rb",
            "lib/.rubocop.yml",
            ".git/config",
            ".pryrc",
            "app/.pryrc",
            "app/gems.rb",
            "Rakefile",
            "Gemfile",
            "tmp/a/b.rb",
            "/x/node_modules/a/b.js",
            "db/schema.rb",
            "lib/bd/x.rb",
            "lib/c/x.rb",
            "é/ü.rb",
        ];
        for pattern in patterns {
            let compiled = PathPattern::new(pattern);
            for path in paths {
                assert_eq!(
                    compiled.matches(&MatchPath::new(path)),
                    match_path(pattern, path),
                    "{pattern} vs {path}"
                );
            }
        }
    }
}
