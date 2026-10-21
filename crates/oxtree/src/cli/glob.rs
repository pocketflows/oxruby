//! Port of Ruby's `Dir.glob(pattern)` (`dir.c`: `glob_helper`), as stree's
//! CLI calls it: no flags, so braces are expanded, wildcards don't match a
//! leading `.`, and `**/` neither enters hidden directories nor follows
//! symlinks. Entries are visited in sorted order, depth first, which is the
//! order Ruby 3 returns them in.
//!
//! Not ported: on case-insensitive file systems Ruby rewrites a literal last
//! component to its on-disk spelling; here it is returned as typed.

use std::fs;
use std::path::Path;

use super::fnmatch::{FNM_PATHNAME, brace_expand, fnmatch};

#[derive(Debug, Clone, PartialEq)]
enum Segment {
    /// `**` followed by `/`.
    Recursive,
    /// A segment with `*`, `?` or `[`, matched against directory entries.
    Magic(String),
    /// A literal segment, with backslash escapes removed.
    Plain(String),
}

/// `Dir.glob(pattern)`, relative to `base` (the working directory). Paths are
/// returned the way Ruby spells them: relative patterns give relative paths.
pub fn glob(pattern: &str, base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    brace_expand(pattern, 0, &mut |p| {
        glob_one(p, base, &mut out);
        false
    });
    out
}

fn glob_one(pattern: &str, base: &Path, out: &mut Vec<String>) {
    if pattern.is_empty() {
        return;
    }
    let (root, rest) = match pattern.strip_prefix('/') {
        Some(rest) => ("/".to_string(), rest),
        None => (String::new(), pattern),
    };
    let dir_only = rest.ends_with('/');
    let mut segments = Vec::new();
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    for (i, part) in parts.iter().enumerate() {
        if *part == "**" && (i + 1 < parts.len() || dir_only) {
            if segments.last() != Some(&Segment::Recursive) {
                segments.push(Segment::Recursive);
            }
        } else if has_magic(part) {
            segments.push(Segment::Magic(part.to_string()));
        } else {
            segments.push(Segment::Plain(remove_backslashes(part)));
        }
    }
    let walker = Walker {
        segments: &segments,
        base,
        dir_only,
    };
    walker.helper(&root, &[0], true, out);
}

struct Walker<'a> {
    segments: &'a [Segment],
    base: &'a Path,
    dir_only: bool,
}

impl Walker<'_> {
    fn disk(&self, path: &str) -> std::path::PathBuf {
        if path.is_empty() {
            self.base.to_path_buf()
        } else {
            self.base.join(path)
        }
    }

    /// Visits `path` with the pattern positions in `states`. `known` is true
    /// when `path` came from a directory listing (or is the starting point).
    fn helper(&self, path: &str, states: &[usize], known: bool, out: &mut Vec<String>) {
        let n = self.segments.len();
        if states.contains(&n) && !path.is_empty() && path != "/" {
            let meta = fs::metadata(self.disk(path));
            let exists = if known {
                true
            } else {
                fs::symlink_metadata(self.disk(path)).is_ok()
            };
            if self.dir_only {
                if meta.map(|m| m.is_dir()).unwrap_or(false) {
                    out.push(format!("{path}/"));
                }
            } else if exists {
                out.push(path.to_string());
            }
        }
        let pending: Vec<usize> = states.iter().copied().filter(|&s| s < n).collect();
        if pending.is_empty() {
            return;
        }
        let needs_listing = pending
            .iter()
            .any(|&s| matches!(self.segments[s], Segment::Recursive | Segment::Magic(_)));
        if !needs_listing {
            for &s in &pending {
                if let Segment::Plain(name) = &self.segments[s] {
                    self.helper(&join(path, name), &[s + 1], false, out);
                }
            }
            return;
        }
        let Ok(entries) = fs::read_dir(self.disk(path)) else {
            return;
        };
        let mut names: Vec<(String, bool)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let real_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                Some((name, real_dir))
            })
            .collect();
        names.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        for (name, real_dir) in names {
            let mut next = Vec::new();
            for &s in &pending {
                match &self.segments[s] {
                    Segment::Recursive => {
                        if real_dir && !name.starts_with('.') {
                            push_unique(&mut next, s);
                        }
                        if let Some(seg) = self.segments.get(s + 1)
                            && segment_matches(seg, &name)
                        {
                            push_unique(&mut next, s + 2);
                        } else if s + 1 == n && real_dir && !name.starts_with('.') {
                            push_unique(&mut next, s + 1);
                        }
                    }
                    seg => {
                        if segment_matches(seg, &name) {
                            push_unique(&mut next, s + 1);
                        }
                    }
                }
            }
            if !next.is_empty() {
                self.helper(&join(path, &name), &next, true, out);
            }
        }
    }
}

fn push_unique(v: &mut Vec<usize>, s: usize) {
    if !v.contains(&s) {
        v.push(s);
    }
}

fn segment_matches(segment: &Segment, name: &str) -> bool {
    match segment {
        Segment::Recursive => false,
        Segment::Magic(p) => fnmatch(p, name, FNM_PATHNAME),
        Segment::Plain(p) => p == name,
    }
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else if path.ends_with('/') {
        format!("{path}{name}")
    } else {
        format!("{path}/{name}")
    }
}

fn has_magic(segment: &str) -> bool {
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        match c {
            '*' | '?' | '[' => return true,
            '\\' => {
                chars.next();
            }
            _ => {}
        }
    }
    false
}

fn remove_backslashes(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("oxtree-glob-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for d in ["lib/x", ".hid", "B"] {
            fs::create_dir_all(dir.join(d)).unwrap();
        }
        for f in [
            "a.rb",
            "z.rb",
            "lib/b.rb",
            "lib/x/c.rb",
            ".hid/d.rb",
            ".e.rb",
            "B/q.rb",
        ] {
            fs::write(dir.join(f), "").unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("lib", dir.join("lnk")).unwrap();
        dir
    }

    // Expected values from Ruby 3.3's Dir.glob on the same tree.
    #[test]
    fn globs_like_ruby() {
        let dir = tree();
        let g = |p: &str| glob(p, &dir);
        assert_eq!(
            g("**/*.rb"),
            ["B/q.rb", "a.rb", "lib/b.rb", "lib/x/c.rb", "z.rb"]
        );
        assert_eq!(g("*"), ["B", "a.rb", "lib", "lnk", "z.rb"]);
        assert_eq!(g("lib"), ["lib"]);
        assert_eq!(g("./lib/*.rb"), ["./lib/b.rb"]);
        assert_eq!(g("{z,a}.rb"), ["z.rb", "a.rb"]);
        assert_eq!(g("lib/../a.rb"), ["lib/../a.rb"]);
        assert_eq!(g("**/"), ["B/", "lib/", "lib/x/"]);
        assert_eq!(g("lnk/**/*.rb"), ["lnk/b.rb", "lnk/x/c.rb"]);
        assert_eq!(g("**/lib/*.rb"), ["lib/b.rb"]);
        assert_eq!(g("missing.rb"), Vec::<String>::new());
        assert_eq!(g(".hid/*.rb"), [".hid/d.rb"]);
        assert_eq!(g("**"), ["B", "a.rb", "lib", "lnk", "z.rb"]);
        assert_eq!(g("lib/**"), ["lib/b.rb", "lib/x"]);
        assert_eq!(g("**/x"), ["lib/x"]);
        assert_eq!(g("lnk"), ["lnk"]);
        assert_eq!(g("a.rb/"), Vec::<String>::new());
        let abs = format!("{}/a.rb", dir.display());
        assert_eq!(glob(&abs, Path::new("/nonexistent")), [abs]);
        let _ = fs::remove_dir_all(&dir);
    }
}
