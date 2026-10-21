//! Port of Ruby's `File.fnmatch` (`dir.c`: `fnmatch`, `fnmatch_helper`,
//! `bracket` and `ruby_brace_expand`), for the flags the CLIs use:
//! `FNM_PATHNAME`, `FNM_DOTMATCH`, `FNM_NOESCAPE` and `FNM_EXTGLOB`.
//! Matching is case-sensitive (`FNM_CASEFOLD` is never passed).

pub const FNM_NOESCAPE: u32 = 0x01;
pub const FNM_PATHNAME: u32 = 0x02;
pub const FNM_DOTMATCH: u32 = 0x04;
pub const FNM_EXTGLOB: u32 = 0x10;

/// `File.fnmatch(pattern, path, flags)`
pub fn fnmatch(pattern: &str, path: &str, flags: u32) -> bool {
    if flags & FNM_EXTGLOB != 0 {
        let mut matched = false;
        brace_expand(pattern, flags, &mut |p| {
            if fnmatch_one(p, path, flags) {
                matched = true;
            }
            matched
        });
        matched
    } else {
        fnmatch_one(pattern, path, flags)
    }
}

fn fnmatch_one(pattern: &str, path: &str, flags: u32) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    fnmatch_chars(&p, &s, flags)
}

/// `fnmatch` from dir.c.
fn fnmatch_chars(p: &[char], s: &[char], flags: u32) -> bool {
    let period = flags & FNM_DOTMATCH == 0;
    let pathname = flags & FNM_PATHNAME != 0;
    if !pathname {
        let (mut pi, mut si) = (0, 0);
        return helper(p, &mut pi, s, &mut si, flags);
    }
    let at = |v: &[char], i: usize| v.get(i).copied().unwrap_or('\0');
    let (mut pi, mut si) = (0usize, 0usize);
    let mut ptmp: Option<usize> = None;
    let mut stmp: Option<usize> = None;
    loop {
        if at(p, pi) == '*' && at(p, pi + 1) == '*' && at(p, pi + 2) == '/' {
            loop {
                pi += 3;
                if !(at(p, pi) == '*' && at(p, pi + 1) == '*' && at(p, pi + 2) == '/') {
                    break;
                }
            }
            ptmp = Some(pi);
            stmp = Some(si);
        }
        if helper(p, &mut pi, s, &mut si, flags) {
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
        // failed: try next recursion
        if let (Some(pt), Some(st)) = (ptmp, stmp)
            && !(period && at(s, st) == '.')
        {
            let mut st = st;
            while st < s.len() && s[st] != '/' {
                st += 1;
            }
            if st < s.len() {
                pi = pt;
                st += 1;
                stmp = Some(st);
                si = st;
                continue;
            }
        }
        return false;
    }
}

/// `fnmatch_helper` from dir.c: matches one path segment (the whole string
/// without `FNM_PATHNAME`). Leaves `pi`/`si` where matching stopped.
fn helper(p: &[char], pcur: &mut usize, s: &[char], scur: &mut usize, flags: u32) -> bool {
    let period = flags & FNM_DOTMATCH == 0;
    let pathname = flags & FNM_PATHNAME != 0;
    let escape = flags & FNM_NOESCAPE == 0;
    let at = |v: &[char], i: usize| v.get(i).copied().unwrap_or('\0');
    let isend = |v: &[char], i: usize| i >= v.len() || (pathname && v[i] == '/');
    let unescape = |i: usize| if escape && at(p, i) == '\\' { i + 1 } else { i };

    let mut pi = *pcur;
    let mut si = *scur;
    let mut ptmp: Option<usize> = None;
    let mut stmp: Option<usize> = None;

    macro_rules! ret {
        ($v:expr) => {{
            *pcur = pi;
            *scur = si;
            return $v;
        }};
    }

    if period && at(s, si) == '.' && at(p, unescape(pi)) != '.' {
        ret!(false);
    }

    loop {
        let mut failed = false;
        match at(p, pi) {
            '*' => {
                loop {
                    pi += 1;
                    if at(p, pi) != '*' {
                        break;
                    }
                }
                if isend(p, unescape(pi)) {
                    pi = unescape(pi);
                    ret!(true);
                }
                if isend(s, si) {
                    ret!(false);
                }
                ptmp = Some(pi);
                stmp = Some(si);
                continue;
            }
            '?' => {
                if isend(s, si) {
                    ret!(false);
                }
                pi += 1;
                si += 1;
                continue;
            }
            '[' => {
                if isend(s, si) {
                    ret!(false);
                }
                if let Some(t) = bracket(p, pi + 1, s[si], flags) {
                    pi = t;
                    si += 1;
                    continue;
                }
                failed = true;
            }
            _ => {}
        }
        if !failed {
            // ordinary
            pi = unescape(pi);
            if isend(s, si) {
                ret!(isend(p, pi));
            }
            if isend(p, pi) {
                failed = true;
            } else if p[pi] == s[si] {
                pi += 1;
                si += 1;
                continue;
            } else {
                failed = true;
            }
        }
        debug_assert!(failed);
        // failed: try next '*' position
        if let (Some(pt), Some(st)) = (ptmp, stmp) {
            pi = pt;
            let st = st + 1;
            stmp = Some(st);
            si = st;
            continue;
        }
        ret!(false);
    }
}

/// `bracket` from dir.c. `pi` points just after `[`. Returns the index after
/// the closing `]` when `c` matches.
fn bracket(p: &[char], mut pi: usize, c: char, flags: u32) -> Option<usize> {
    let escape = flags & FNM_NOESCAPE == 0;
    let at = |i: usize| p.get(i).copied().unwrap_or('\0');
    if pi >= p.len() {
        return None;
    }
    let mut ok = false;
    let mut not = false;
    if at(pi) == '!' || at(pi) == '^' {
        not = true;
        pi += 1;
    }
    while at(pi) != ']' {
        let mut t1 = pi;
        if escape && at(t1) == '\\' {
            t1 += 1;
        }
        if t1 >= p.len() {
            return None;
        }
        pi = t1 + 1;
        if pi >= p.len() {
            return None;
        }
        if at(pi) == '-' && at(pi + 1) != ']' {
            let mut t2 = pi + 1;
            if escape && at(t2) == '\\' {
                t2 += 1;
            }
            if t2 >= p.len() {
                return None;
            }
            pi = t2 + 1;
            if ok {
                continue;
            }
            if p[t1] == c || p[t2] == c {
                ok = true;
                continue;
            }
            if c < p[t1] {
                continue;
            }
            if c > p[t2] {
                continue;
            }
        } else {
            if ok {
                continue;
            }
            if p[t1] == c {
                ok = true;
                continue;
            }
            continue;
        }
        ok = true;
    }
    if ok == not { None } else { Some(pi + 1) }
}

/// `ruby_brace_expand`: calls `f` with each expansion, in order, until it
/// returns true. A `{` without a matching `}` expands to nothing.
pub fn brace_expand(pattern: &str, flags: u32, f: &mut dyn FnMut(&str) -> bool) -> bool {
    let escape = flags & FNM_NOESCAPE == 0;
    let p: Vec<char> = pattern.chars().collect();
    let mut lbrace: Option<usize> = None;
    let mut rbrace: Option<usize> = None;
    let mut nest = 0i32;
    let mut i = 0;
    while i < p.len() {
        if p[i] == '{' {
            if nest == 0 {
                lbrace = Some(i);
            }
            nest += 1;
        }
        if p[i] == '}' && lbrace.is_some() {
            nest -= 1;
            if nest == 0 {
                rbrace = Some(i);
                break;
            }
        }
        if p[i] == '\\' && escape {
            i += 1;
            if i >= p.len() {
                break;
            }
        }
        i += 1;
    }
    match (lbrace, rbrace) {
        (Some(l), Some(r)) => {
            let prefix: String = p[..l].iter().collect();
            let suffix: String = p[r + 1..].iter().collect();
            let mut i = l;
            while i < r {
                i += 1;
                let t = i;
                let mut nest = 0i32;
                while i < r && !(p[i] == ',' && nest == 0) {
                    if p[i] == '{' {
                        nest += 1;
                    }
                    if p[i] == '}' {
                        nest -= 1;
                    }
                    if p[i] == '\\' && escape {
                        i += 1;
                        if i == r {
                            break;
                        }
                    }
                    i += 1;
                }
                let middle: String = p[t..i.min(r)].iter().collect();
                let expanded = format!("{prefix}{middle}{suffix}");
                if brace_expand(&expanded, flags, f) {
                    return true;
                }
            }
            false
        }
        (None, None) => f(pattern),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PE: u32 = FNM_PATHNAME | FNM_EXTGLOB;

    // Expected values from Ruby 3.3: File.fnmatch(pattern, path, flags).
    #[test]
    fn matches_ruby() {
        let cases: &[(&str, &str, u32, bool)] = &[
            ("db/**/*.rb", "db/a/b.rb", 0, true),
            ("db/**/*.rb", "db/b.rb", 0, false),
            ("*/test*", "/tmp/test-x.rb", 0, true),
            ("tmp/*.rb", "tmp/a/b.rb", 0, true),
            ("*", "x/.y", 0, true),
            ("*", ".y", 0, false),
            ("vendor/**/*", "vendor/a", 0, false),
            ("vendor/**/*", "vendor/a/b", 0, true),
            (
                "app/views/icons/lucide/*",
                "app/views/icons/lucide/x/y.html.erb",
                0,
                true,
            ),
            ("{a,b}.rb", "a.rb", 0, false),
            ("**/*.rb", ".github/a.rb", PE, false),
            ("**/*.rb", "a/.b.rb", PE, false),
            ("**/*.rb", "a.rb", PE, true),
            ("**/*.rb", "a/b/c.rb", PE, true),
            ("**/*.rb", "a/.b/c.rb", PE, false),
            ("Rakefile", "Rakefile", PE, true),
            ("Rakefile", "lib/Rakefile", PE, false),
            ("**/*.html.erb", "app/views/a.html.erb", PE, true),
            ("**/*.{js,css}.erb", "app/a.css.erb", PE, true),
            ("**/*.{js,css}.erb", "app/a.json.erb", PE, false),
            ("{a,b}.rb", "b.rb", PE, true),
            (
                "bin/generate-openapi-spec",
                "bin/generate-openapi-spec",
                PE,
                true,
            ),
            ("*.rb", "a/b.rb", PE, false),
            ("a?c", "abc", 0, true),
            ("a[b-d]c", "acc", 0, true),
            ("a[!b-d]c", "acc", 0, false),
            ("a[!b-d]c", "aec", 0, true),
            ("a\\*c", "a*c", 0, true),
            ("a\\*c", "abc", 0, false),
            ("a*c", "a/c", PE, false),
            ("a[/]c", "a/c", PE, false),
            ("a*b*c", "axxbyyc", 0, true),
            ("a*b*c", "axxbyy", 0, false),
            ("**/foo", "a/b/foo", PE, true),
            ("**/foo", "foo", PE, true),
            ("**/foo", ".a/foo", PE, false),
        ];
        for &(pattern, path, flags, expected) in cases {
            assert_eq!(
                fnmatch(pattern, path, flags),
                expected,
                "fnmatch({pattern:?}, {path:?}, {flags})"
            );
        }
    }

    #[test]
    fn expands_braces() {
        let mut out = Vec::new();
        brace_expand("{z,a{b,c}}.rb", 0, &mut |p| {
            out.push(p.to_string());
            false
        });
        assert_eq!(out, ["z.rb", "ab.rb", "ac.rb"]);
        out.clear();
        brace_expand("{a.rb", 0, &mut |p| {
            out.push(p.to_string());
            false
        });
        assert!(out.is_empty());
    }
}
