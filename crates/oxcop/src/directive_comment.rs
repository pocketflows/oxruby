//! Port of `lib/rubocop/directive_comment.rb`: a `# rubocop:disable ...`
//! style comment.

use std::sync::LazyLock;

use onig::Regex;

use crate::processed_source::Range;

pub const LINT_DEPARTMENT: &str = "Lint";
pub const LINT_REDUNDANT_DIRECTIVE_COP: &str = "Lint/RedundantCopDisableDirective";
pub const LINT_SYNTAX_COP: &str = "Lint/Syntax";
pub const STYLE_DISABLE_COPS_DIRECTIVE_COP: &str = "Style/DisableCopsWithinSourceCodeDirective";
pub const TRAILING_COMMENT_MARKER: &str = "--";

const COP_NAME_PATTERN: &str = r"([A-Za-z]\w+/)*(?:[A-Za-z]\w+)";
const COP_NAME_PATTERN_NC: &str = r"(?:[A-Za-z]\w+/)*[A-Za-z]\w+";

fn modes_pattern() -> String {
    let mut modes = vec![
        "disable",
        "enable",
        "todo",
        "push",
        "pop",
        "disable-next",
        "todo-next",
        "enable-next",
        "next",
    ];
    // `sort_by { |mode| -mode.length }`
    modes.sort_by_key(|m| std::cmp::Reverse(m.len()));
    modes.join("|")
}

fn spaces(pattern: &str) -> String {
    pattern.replace(' ', r"\s*")
}

static DIRECTIVE_HEADER_PATTERN: LazyLock<String> =
    LazyLock::new(|| format!(r"# rubocop : ((?:{}))\b", modes_pattern()));

pub static DIRECTIVE_MARKER_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&spaces("# rubocop : ")).unwrap());

pub static DIRECTIVE_COMMENT_REGEXP: LazyLock<Regex> = LazyLock::new(|| {
    let cop_names = format!("(?:{COP_NAME_PATTERN} , )*{COP_NAME_PATTERN}");
    let cops = format!("(all|{cop_names})");
    let push_pop_args = format!(r"([+\-]{COP_NAME_PATTERN_NC}(?:\s+[+\-]{COP_NAME_PATTERN_NC})*)");
    Regex::new(&spaces(&format!(
        r"{}(?:\s+{cops}|\s+{push_pop_args})?",
        *DIRECTIVE_HEADER_PATTERN
    )))
    .unwrap()
});

static MALFORMED_DIRECTIVE_WITHOUT_COP_NAME_REGEXP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&spaces(&format!(r"\A{}\s*\z", *DIRECTIVE_HEADER_PATTERN))).unwrap()
});

static PRE_MATCH_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A#\s*\z").unwrap());

/// The names of all registered cops, for `all` and department names.
pub trait CopRegistry {
    fn names(&self) -> &[String];
    fn is_department(&self, name: &str) -> bool;
    fn names_for_department(&self, department: &str) -> Vec<String>;
}

#[derive(Debug, Clone)]
pub struct DirectiveComment<'src> {
    pub comment_text: &'src str,
    pub comment_range: Range,
    /// `comment.source_range.line`; `i64::MIN` for directives injected for
    /// cops disabled in the configuration.
    pub line_number: i64,
    match_start: Option<usize>,
    match_end: Option<usize>,
    pub mode: Option<String>,
    pub cops: Option<String>,
}

impl<'src> DirectiveComment<'src> {
    pub fn new(text: &'src str, range: Range, line_number: i64) -> Self {
        let mut mode = None;
        let mut cops = None;
        let mut match_start = None;
        let mut match_end = None;
        if let Some(captures) = DIRECTIVE_COMMENT_REGEXP.captures(text) {
            let (start, end) = captures.pos(0).unwrap();
            if !PRE_MATCH_COMMENT.is_match(&text[..start]) {
                match_start = Some(start);
                match_end = Some(end);
                mode = captures.at(1).map(str::to_owned);
                cops = captures.at(2).or_else(|| captures.at(5)).map(str::to_owned);
            }
        }
        Self {
            comment_text: text,
            comment_range: range,
            line_number,
            match_start,
            match_end,
            mode,
            cops,
        }
    }

    pub fn has_match(&self) -> bool {
        self.match_start.is_some()
    }

    pub fn start_with_marker(&self) -> bool {
        DIRECTIVE_MARKER_REGEXP
            .find(self.comment_text)
            .is_some_and(|(start, _)| start == 0)
    }

    fn post_match(&self) -> &'src str {
        self.match_end.map_or("", |end| &self.comment_text[end..])
    }

    pub fn malformed(&self) -> bool {
        if !self.start_with_marker() || !self.has_match() {
            return true;
        }
        if self.missing_cop_name() || self.invalid_signed_args() {
            return true;
        }
        let tail = self.post_match().trim_start();
        !(tail.is_empty() || tail.starts_with(TRAILING_COMMENT_MARKER))
    }

    pub fn missing_cop_name(&self) -> bool {
        if self.push() || self.pop() {
            return false;
        }
        MALFORMED_DIRECTIVE_WITHOUT_COP_NAME_REGEXP.is_match(self.comment_text)
    }

    pub fn invalid_signed_args(&self) -> bool {
        if self.pop() {
            return self.cops.as_ref().is_some_and(|c| !c.is_empty());
        }
        if !(self.push() || self.next()) {
            return false;
        }
        let Some(cops) = &self.cops else {
            return false;
        };
        cops.split_whitespace()
            .any(|spec| !spec.starts_with('+') && !spec.starts_with('-'))
    }

    pub fn reason(&self) -> Option<String> {
        self.match_start?;
        let tail = self.post_match().trim_start();
        let reason = tail.strip_prefix(TRAILING_COMMENT_MARKER)?.trim();
        (!reason.is_empty()).then(|| reason.to_owned())
    }

    /// `single_line?`: the directive does not start the comment.
    pub fn single_line(&self) -> bool {
        DIRECTIVE_COMMENT_REGEXP
            .find(self.comment_text)
            .is_none_or(|(start, _)| start != 0)
    }

    /// The range of the directive itself within the comment.
    pub fn range(&self) -> Option<Range> {
        let (start, end) = DIRECTIVE_COMMENT_REGEXP.find(self.comment_text)?;
        let begin = self.comment_range.begin_pos;
        Some(Range::new(begin + start, begin + end))
    }

    fn mode_is(&self, modes: &[&str]) -> bool {
        self.mode.as_deref().is_some_and(|m| modes.contains(&m))
    }

    pub fn disabled(&self) -> bool {
        self.mode_is(&["disable", "todo"]) || self.disable_next()
    }

    pub fn disable_next(&self) -> bool {
        self.mode_is(&["disable-next", "todo-next"])
    }

    pub fn enabled(&self) -> bool {
        self.mode_is(&["enable"]) || self.enable_next()
    }

    pub fn enable_next(&self) -> bool {
        self.mode_is(&["enable-next"])
    }

    pub fn push(&self) -> bool {
        self.mode_is(&["push"])
    }

    pub fn pop(&self) -> bool {
        self.mode_is(&["pop"])
    }

    pub fn next(&self) -> bool {
        self.mode_is(&["next"])
    }

    pub fn enabled_all(&self) -> bool {
        !self.disabled() && self.all_cops()
    }

    pub fn disabled_all(&self) -> bool {
        self.disabled() && self.all_cops()
    }

    pub fn all_cops(&self) -> bool {
        self.cops.as_deref() == Some("all")
    }

    /// `signed_args`: `{'+' => [...], '-' => [...]}` for push/next.
    pub fn signed_args(&self) -> Vec<(char, Vec<String>)> {
        let mut args: Vec<(char, Vec<String>)> = Vec::new();
        if !(self.push() || self.next()) {
            return args;
        }
        let Some(cops) = &self.cops else {
            return args;
        };
        for spec in cops.split_whitespace() {
            let mut chars = spec.chars();
            let Some(op) = chars.next() else { continue };
            if op != '+' && op != '-' {
                continue;
            }
            let name = chars.as_str().to_owned();
            match args.iter_mut().find(|(o, _)| *o == op) {
                Some((_, names)) => names.push(name),
                None => args.push((op, vec![name])),
            }
        }
        args
    }

    /// `cop_names`: may be expanded from `all` or department names.
    pub fn cop_names(&self, registry: &dyn CopRegistry) -> Vec<String> {
        if self.all_cops() {
            exclude_lint_department_cops(registry.names().to_vec())
        } else {
            self.parsed_cop_names(registry)
        }
    }

    pub fn raw_cop_names(&self) -> Vec<String> {
        match &self.cops {
            None => Vec::new(),
            Some(cops) => split_cop_list(cops),
        }
    }

    pub fn department_names(&self, registry: &dyn CopRegistry) -> Vec<String> {
        self.raw_cop_names()
            .into_iter()
            .filter(|cop| registry.is_department(cop))
            .collect()
    }

    pub fn directive_count(&self) -> usize {
        if self.push() || self.next() {
            return self.signed_args().iter().map(|(_, n)| n.len()).sum();
        }
        self.raw_cop_names().len()
    }

    fn parsed_cop_names(&self, registry: &dyn CopRegistry) -> Vec<String> {
        let mut cops = Vec::new();
        for name in self.raw_cop_names() {
            if registry.is_department(&name) {
                let names = registry.names_for_department(&name);
                if name == LINT_DEPARTMENT {
                    cops.extend(exclude_lint_department_cops(names));
                } else {
                    cops.extend(names);
                }
            } else {
                cops.push(name);
            }
        }
        cops.retain(|c| c != LINT_SYNTAX_COP);
        cops
    }
}

/// `(cops || '').split(/,\s*/)`
fn split_cop_list(cops: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cops;
    loop {
        match rest.find(',') {
            Some(i) => {
                out.push(rest[..i].to_owned());
                rest = rest[i + 1..].trim_start_matches([' ', '\t', '\n', '\r', '\x0c', '\x0b']);
            }
            None => {
                out.push(rest.to_owned());
                break;
            }
        }
    }
    // Ruby's split drops trailing empty strings.
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn exclude_lint_department_cops(mut cops: Vec<String>) -> Vec<String> {
    cops.retain(|c| c != LINT_REDUNDANT_DIRECTIVE_COP && c != LINT_SYNTAX_COP);
    cops
}
