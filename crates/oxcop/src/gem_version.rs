//! Port of RubyGems' `Gem::Version` and `Gem::Requirement`
//! (`rubygems/version.rb`, `rubygems/requirement.rb`), as far as RuboCop
//! uses them: comparing versions and `Gem::Requirement#satisfied_by?`
//! (`TargetRuby::GemspecFile`, and `requires_gem` for plugin ports).

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

/// `Gem::Version::VERSION_PATTERN`.
const VERSION_PATTERN: &str = r"[0-9]+(?:\.[0-9a-zA-Z]+)*(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?";

static ANCHORED_VERSION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\A\s*({VERSION_PATTERN})?\s*\z")).unwrap());

/// `Gem::Requirement::PATTERN`.
static REQUIREMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\A\s*(=|!=|>=|<=|>|<|~>)?\s*({VERSION_PATTERN})\s*\z"
    ))
    .unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Number(u64),
    String(String),
}

/// `Gem::Version`.
#[derive(Debug, Clone)]
pub struct Version {
    version: String,
    segments: Vec<Segment>,
}

impl Version {
    /// `Gem::Version.new`: `None` where RubyGems raises "Malformed version
    /// number string".
    pub fn new(version: &str) -> Option<Self> {
        if !ANCHORED_VERSION_PATTERN.is_match(version) {
            return None;
        }
        let version = version.trim();
        // An empty string is version 0.
        let version = if version.is_empty() { "0" } else { version };
        let version = version.replace('-', ".pre.");
        let segments = segment_scan(&version);
        Some(Self { version, segments })
    }

    /// `Gem::Version#to_s`.
    pub fn as_str(&self) -> &str {
        &self.version
    }

    /// `Gem::Version#segments`, numeric segments only (`[7, 1, 2]` for
    /// `7.1.2.beta`), for callers like `gem_version_to_major_minor_float`.
    pub fn numeric_segments(&self) -> Vec<u64> {
        self.segments
            .iter()
            .map_while(|s| match s {
                Segment::Number(n) => Some(*n),
                Segment::String(_) => None,
            })
            .collect()
    }

    /// `Gem::Version#prerelease?`.
    pub fn is_prerelease(&self) -> bool {
        self.version.chars().any(|c| c.is_ascii_alphabetic())
    }

    /// `Gem::Version#release`.
    pub fn release(&self) -> Self {
        if !self.is_prerelease() {
            return self.clone();
        }
        let mut segments = self.segments.clone();
        while segments.iter().any(|s| matches!(s, Segment::String(_))) {
            segments.pop();
        }
        Self::from_segments(segments)
    }

    /// `Gem::Version#bump`.
    pub fn bump(&self) -> Self {
        let mut segments = self.segments.clone();
        while segments.iter().any(|s| matches!(s, Segment::String(_))) {
            segments.pop();
        }
        if segments.len() > 1 {
            segments.pop();
        }
        if let Some(Segment::Number(last)) = segments.last_mut() {
            *last += 1;
        }
        Self::from_segments(segments)
    }

    fn from_segments(segments: Vec<Segment>) -> Self {
        let version = segments
            .iter()
            .map(|s| match s {
                Segment::Number(n) => n.to_string(),
                Segment::String(s) => s.clone(),
            })
            .collect::<Vec<_>>()
            .join(".");
        Self::new(&version).unwrap_or(Self {
            version,
            segments: Vec::new(),
        })
    }

    /// `Gem::Version#canonical_segments`: trailing zeros dropped from the
    /// numeric and the string part.
    fn canonical_segments(&self) -> Vec<Segment> {
        let split = self
            .segments
            .iter()
            .position(|s| matches!(s, Segment::String(_)))
            .unwrap_or(self.segments.len());
        let (numeric, string) = self.segments.split_at(split);
        let trim = |part: &[Segment]| {
            let mut part = part.to_vec();
            while part.last() == Some(&Segment::Number(0)) {
                part.pop();
            }
            part
        };
        let mut canonical = trim(numeric);
        canonical.extend(trim(string));
        canonical
    }
}

/// `@version.scan(/[0-9]+|[a-z]+/i)`, numbers as integers.
fn segment_scan(version: &str) -> Vec<Segment> {
    static SEGMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+|[a-zA-Z]+").unwrap());
    SEGMENT
        .find_iter(version)
        .map(|m| match m.as_str().parse() {
            Ok(n) => Segment::Number(n),
            Err(_) => Segment::String(m.as_str().to_owned()),
        })
        .collect()
}

impl Ord for Version {
    /// `Gem::Version#<=>`.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.version == other.version {
            return Ordering::Equal;
        }
        let lhs = self.canonical_segments();
        let rhs = other.canonical_segments();
        if lhs == rhs {
            return Ordering::Equal;
        }
        let zero = Segment::Number(0);
        for i in 0..lhs.len().max(rhs.len()) {
            let l = lhs.get(i).unwrap_or(&zero);
            let r = rhs.get(i).unwrap_or(&zero);
            let ordering = match (l, r) {
                _ if l == r => continue,
                (Segment::String(_), Segment::Number(_)) => Ordering::Less,
                (Segment::Number(_), Segment::String(_)) => Ordering::Greater,
                (Segment::Number(a), Segment::Number(b)) => a.cmp(b),
                (Segment::String(a), Segment::String(b)) => a.cmp(b),
            };
            return ordering;
        }
        Ordering::Equal
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Pessimistic,
}

/// `Gem::Requirement`.
#[derive(Debug, Clone)]
pub struct Requirement {
    requirements: Vec<(Op, Version)>,
}

impl Requirement {
    /// `Gem::Requirement.new(*requirements)`: `None` where RubyGems raises
    /// `BadRequirementError`. No requirements means `>= 0`.
    pub fn new<S: AsRef<str>>(requirements: &[S]) -> Option<Self> {
        let mut parsed = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for requirement in requirements {
            let requirement = requirement.as_ref();
            if seen.contains(&requirement) {
                continue;
            }
            seen.push(requirement);
            parsed.push(Self::parse(requirement)?);
        }
        if parsed.is_empty() {
            parsed.push((Op::Ge, Version::new("0")?));
        }
        Some(Self {
            requirements: parsed,
        })
    }

    /// `Gem::Requirement.parse`.
    fn parse(requirement: &str) -> Option<(Op, Version)> {
        let captures = REQUIREMENT_PATTERN.captures(requirement)?;
        let op = match captures.get(1).map(|m| m.as_str()) {
            None | Some("=") => Op::Eq,
            Some("!=") => Op::Ne,
            Some(">") => Op::Gt,
            Some("<") => Op::Lt,
            Some(">=") => Op::Ge,
            Some("<=") => Op::Le,
            Some(_) => Op::Pessimistic,
        };
        Some((op, Version::new(&captures[2])?))
    }

    /// `Gem::Requirement#satisfied_by?`.
    pub fn is_satisfied_by(&self, version: &Version) -> bool {
        self.requirements.iter().all(|(op, required)| match op {
            Op::Eq => version == required,
            Op::Ne => version != required,
            Op::Gt => version > required,
            Op::Lt => version < required,
            Op::Ge => version >= required,
            Op::Le => version <= required,
            Op::Pessimistic => version >= required && version.release() < required.bump(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(version: &str) -> Version {
        Version::new(version).unwrap()
    }

    fn satisfied(requirements: &[&str], version: &str) -> bool {
        Requirement::new(requirements)
            .unwrap()
            .is_satisfied_by(&v(version))
    }

    #[test]
    fn versions_compare_as_rubygems_does() {
        assert_eq!(v("1.0"), v("1"));
        assert_eq!(v("1.0.0"), v("1"));
        assert!(v("1.10") > v("1.9"));
        assert!(v("1.0.a") < v("1.0"));
        assert!(v("1.0.b") > v("1.0.a"));
        assert!(v("2.7.99") > v("2.7.1"));
        assert_eq!(v("1.0-rc1").as_str(), "1.0.pre.rc1");
        assert!(Version::new("junk").is_none());
        assert_eq!(v("7.1.2.beta").numeric_segments(), vec![7, 1, 2]);
    }

    #[test]
    fn requirements_are_satisfied_as_rubygems_does() {
        assert!(satisfied(&[">= 2.7"], "2.7.99"));
        assert!(!satisfied(&[">= 3.0"], "2.7.99"));
        assert!(satisfied(&["~> 3.1"], "3.9.99"));
        assert!(!satisfied(&["~> 3.1"], "4.0.99"));
        assert!(satisfied(&["~> 3.1.0"], "3.1.99"));
        assert!(!satisfied(&["~> 3.1.0"], "3.2.99"));
        assert!(satisfied(&[">= 2.6", "< 3.3"], "3.2.99"));
        assert!(!satisfied(&[">= 2.6", "< 3.3"], "3.3.99"));
        assert!(satisfied(&["3.2.99"], "3.2.99"));
        assert!(satisfied(&["!= 3.0.99"], "3.1.99"));
        assert!(satisfied(&[] as &[&str], "0.1"));
        assert!(Requirement::new(&["> junk"]).is_none());
    }
}
