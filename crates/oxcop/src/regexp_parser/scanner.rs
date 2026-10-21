//! Port of `regexp_parser`'s `Regexp::Scanner` (`lib/regexp_parser/scanner/
//! scanner.rl`, `char_type.rl` and `property.rl`, regexp_parser 2.13.1).
//!
//! The Ragel machines are written out by hand. Each machine is a longest-
//! match scanner: at every position the longest pattern wins, ties going to
//! the pattern listed first, as in Ragel's `|* ... *|`. Token positions are
//! in characters, as the Ruby scanner emits them.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::regex;

/// An error the Ruby scanner raises (`ScannerError`, `PrematureEndError`,
/// `ValidationError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError;

type Result<T> = std::result::Result<T, ScanError>;

/// `[type, token, text, ts, te]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedToken {
    pub ty: &'static str,
    pub token: String,
    pub text: String,
    pub ts: usize,
    pub te: usize,
}

const POSIX_CLASSES: &[&str] = &[
    "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print", "punct",
    "space", "upper", "word", "xdigit",
];

fn parse_prop_map(csv: &'static str) -> HashMap<&'static str, &'static str> {
    csv.lines()
        .filter_map(|line| line.split_once(','))
        .collect()
}

static SHORT_PROP_MAP: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| parse_prop_map(include_str!("properties/short.csv")));
static LONG_PROP_MAP: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| parse_prop_map(include_str!("properties/long.csv")));

/// Ragel's `space`: `[\t\v\f\n\r ]`.
fn is_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}

fn is_meta_char(b: u8) -> bool {
    matches!(
        b,
        b'.' | b'\\'
            | b'|'
            | b'{'
            | b'}'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'^'
            | b'$'
            | b'?'
            | b'*'
            | b'+'
    )
}

/// `ascii_print -- space`: printable ASCII that is not a meta character,
/// `#` or a space.
fn is_literal_print(b: u8) -> bool {
    (0x21..=0x7e).contains(&b) && !is_meta_char(b) && b != b'#'
}

fn is_ascii_nonprint(b: u8) -> bool {
    (0x01..=0x1f).contains(&b) || b == 0x7f
}

fn is_xdigit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

struct Spacing {
    free_spacing: bool,
    depth: i64,
}

struct Scanner<'s> {
    src: &'s str,
    b: &'s [u8],
    tokens: Vec<ScannedToken>,
    literal_run: Option<String>,
    char_pos: usize,
    prev_token: Option<(&'static str, String)>,
    free_spacing: bool,
    spacing_stack: Vec<Spacing>,
    binary: bool,
    set_depth: i64,
    group_depth: i64,
    capturing_group_count: i64,
    conditional_stack: Vec<i64>,
}

/// `Regexp::Scanner.scan(input, options:)`: `extended` is the `x` option,
/// `binary` the `n` option (`Encoding::BINARY`).
pub fn scan(input: &str, extended: bool, binary: bool) -> Result<Vec<ScannedToken>> {
    let mut scanner = Scanner {
        src: input,
        b: input.as_bytes(),
        tokens: Vec::new(),
        literal_run: None,
        char_pos: 0,
        prev_token: None,
        free_spacing: extended,
        spacing_stack: vec![Spacing {
            free_spacing: extended,
            depth: 0,
        }],
        binary,
        set_depth: 0,
        group_depth: 0,
        capturing_group_count: 0,
        conditional_stack: Vec::new(),
    };
    scanner.main(0)?;
    if scanner.group_depth > 0 || scanner.set_depth > 0 {
        return Err(ScanError);
    }
    if scanner.literal_run.is_some() {
        scanner.emit_literal();
    }
    Ok(scanner.tokens)
}

impl<'s> Scanner<'s> {
    fn at(&self, p: usize) -> Option<u8> {
        self.b.get(p).copied()
    }

    fn copy(&self, ts: usize, te: usize) -> &'s str {
        &self.src[ts..te]
    }

    /// The byte length of the character at `p`.
    fn char_len(&self, p: usize) -> usize {
        self.src[p..].chars().next().map_or(1, char::len_utf8)
    }

    fn emit(&mut self, ty: &'static str, token: &str, text: &str) {
        if self.literal_run.is_some() {
            self.emit_literal();
        }
        let ts = self.char_pos;
        let te = ts + text.chars().count();
        self.prev_token = Some((ty, token.to_owned()));
        self.char_pos = te;
        self.tokens.push(ScannedToken {
            ty,
            token: token.to_owned(),
            text: text.to_owned(),
            ts,
            te,
        });
    }

    fn append_literal(&mut self, ts: usize, te: usize) {
        let text = self.copy(ts, te);
        self.literal_run
            .get_or_insert_with(String::new)
            .push_str(text);
    }

    fn emit_literal(&mut self) {
        let text = self.literal_run.take().unwrap_or_default();
        self.emit("literal", "literal", &text);
    }

    // ---- main --------------------------------------------------------------

    fn main(&mut self, mut p: usize) -> Result<usize> {
        while let Some(c) = self.at(p) {
            p = match c {
                b'.' => {
                    self.emit("meta", "dot", ".");
                    p + 1
                }
                b'|' => {
                    if self.conditional_stack.last() == Some(&self.group_depth) {
                        self.emit("conditional", "separator", "|");
                    } else {
                        self.emit("meta", "alternation", "|");
                    }
                    p + 1
                }
                b'^' => {
                    self.emit("anchor", "bol", "^");
                    p + 1
                }
                b'$' => {
                    self.emit("anchor", "eol", "$");
                    p + 1
                }
                b'\\' => self.main_backslash(p)?,
                b']' | b'}' => {
                    self.append_literal(p, p + 1);
                    p + 1
                }
                b'[' => {
                    self.set_depth += 1;
                    self.emit("set", "open", "[");
                    self.character_set(p + 1)?
                }
                b'(' => self.group_open(p)?,
                b')' => self.group_close(p)?,
                b'?' | b'*' | b'+' => self.quantifier(p),
                b'{' => self.interval(p),
                b'#' => {
                    let mut end = p + 1;
                    while let Some(b) = self.at(end) {
                        end += 1;
                        if b == b'\n' {
                            break;
                        }
                    }
                    if self.free_spacing {
                        let text = self.copy(p, end);
                        self.emit("free_space", "comment", text);
                        end
                    } else {
                        // consume only the pound sign (#) and backtrack to do
                        // regular scanning
                        self.append_literal(p, p + 1);
                        p + 1
                    }
                }
                c if is_space(c) => {
                    let space = self.run(p, is_space);
                    let nonprint = self.run(p, is_ascii_nonprint);
                    if nonprint > space {
                        self.append_literal(p, nonprint);
                        nonprint
                    } else if self.free_spacing {
                        let text = self.copy(p, space);
                        self.emit("free_space", "whitespace", text);
                        space
                    } else {
                        self.append_literal(p, space);
                        space
                    }
                }
                c if is_literal_print(c) => {
                    let end = self.run(p, is_literal_print);
                    self.append_literal(p, end);
                    end
                }
                c if is_ascii_nonprint(c) => {
                    let end = self.run(p, is_ascii_nonprint);
                    self.append_literal(p, end);
                    end
                }
                c if c >= 0x80 => {
                    let mut end = p;
                    while self.at(end).is_some_and(|b| b >= 0x80) {
                        end += self.char_len(end);
                    }
                    self.append_literal(p, end);
                    end
                }
                _ => return Err(ScanError),
            };
        }
        Ok(p)
    }

    /// The end of the run of bytes matching `pred` from `p`.
    fn run(&self, p: usize, pred: impl Fn(u8) -> bool) -> usize {
        let mut end = p;
        while self.at(end).is_some_and(&pred) {
            end += 1;
        }
        end
    }

    fn quantifier(&mut self, p: usize) -> usize {
        let c = self.b[p];
        let end = if matches!(self.at(p + 1), Some(b'?' | b'+')) {
            p + 2
        } else {
            p + 1
        };
        let text = self.copy(p, end);
        let base = match c {
            b'?' => "zero_or_one",
            b'*' => "zero_or_more",
            _ => "one_or_more",
        };
        let token = match self.at(p + 1) {
            Some(b'?') => format!("{base}_reluctant"),
            Some(b'+') => format!("{base}_possessive"),
            _ => base.to_owned(),
        };
        self.emit("quantifier", &token, text);
        end
    }

    /// `quantifier_interval`, or an unmatched `{` as a literal.
    fn interval(&mut self, p: usize) -> usize {
        let digits = |q: usize| self.run(q, |b| b.is_ascii_digit());
        let q = p + 1;
        let d1 = digits(q);
        let end = if d1 > q {
            match self.at(d1) {
                Some(b'}') => Some(d1 + 1),
                Some(b',') => {
                    let d2 = digits(d1 + 1);
                    (self.at(d2) == Some(b'}')).then_some(d2 + 1)
                }
                _ => None,
            }
        } else if self.at(q) == Some(b',') {
            let d2 = digits(q + 1);
            (d2 > q + 1 && self.at(d2) == Some(b'}')).then_some(d2 + 1)
        } else {
            None
        };
        match end {
            Some(end) => {
                let text = self.copy(p, end);
                self.emit("quantifier", "interval", text);
                end
            }
            None => {
                self.append_literal(p, p + 1);
                p + 1
            }
        }
    }

    fn main_backslash(&mut self, p: usize) -> Result<usize> {
        let Some(n) = self.at(p + 1) else {
            return Err(ScanError);
        };
        match n {
            b'K' => {
                self.emit("keep", "mark", "\\K");
                return Ok(p + 2);
            }
            b'A' | b'b' | b'B' | b'z' | b'Z' | b'G' => {
                let token = match n {
                    b'A' => "bos",
                    b'z' => "eos",
                    b'Z' => "eos_ob_eol",
                    b'b' => "word_boundary",
                    b'B' => "nonword_boundary",
                    _ => "match_start",
                };
                let text = self.copy(p, p + 2);
                self.emit("anchor", token, text);
                return Ok(p + 2);
            }
            b'k' | b'g' => {
                if let Some(end) = self.group_ref_body(p + 2) {
                    let text = self.copy(p, end);
                    if n == b'k' {
                        self.emit_group_ref(text)?;
                    } else {
                        self.emit_group_call(text)?;
                    }
                    return Ok(end);
                }
            }
            _ => {}
        }
        self.escape_sequence(p + 1)
    }

    /// `group_ref_body` from `q`: the end of `<...>` or `'...'`.
    fn group_ref_body(&self, q: usize) -> Option<usize> {
        let (open, close) = match self.at(q)? {
            b'<' => (b'<', b'>'),
            b'\'' => (b'\'', b'\''),
            _ => return None,
        };
        let _ = open;
        let start = q + 1;
        let end = start + self.b[start..].iter().position(|&b| b == close)?;
        let body = &self.src[start..end];
        let excluded: &[u8] = if close == b'>' {
            b"!=0123456789->"
        } else {
            b"0123456789-'"
        };
        let valid = body.is_empty()
            || !excluded.contains(&body.as_bytes()[0])
            || regex!(r"\A-?[0-9]+(?:[+\-][0-9]+)?\z").find(body).is_some();
        valid.then_some(end + 1)
    }

    fn emit_group_ref(&mut self, text: &str) -> Result<()> {
        let kind = |c: Option<&str>, ab: &'static str, sq: &'static str| {
            if c == Some("<") { ab } else { sq }
        };
        let open = text.get(2..3);
        let token = if regex!(r"^\\k(.)[^0-9\-][^+\-]*['>]$").find(text).is_some() {
            kind(open, "name_ref_ab", "name_ref_sq")
        } else if regex!(r"^\\k(.)0*[1-9]\d*['>]$").find(text).is_some() {
            kind(open, "number_ref_ab", "number_ref_sq")
        } else if regex!(r"^\\k(.)-0*[1-9]\d*['>]$").find(text).is_some() {
            kind(open, "number_rel_ref_ab", "number_rel_ref_sq")
        } else if regex!(r"^\\k(.)[^0-9\-].*[+\-]\d+['>]$")
            .find(text)
            .is_some()
        {
            kind(open, "name_recursion_ref_ab", "name_recursion_ref_sq")
        } else if regex!(r"^\\k(.)-?0*[1-9]\d*[+\-]\d+['>]$")
            .find(text)
            .is_some()
        {
            kind(open, "number_recursion_ref_ab", "number_recursion_ref_sq")
        } else {
            return Err(ScanError);
        };
        self.emit("backref", token, text);
        Ok(())
    }

    fn emit_group_call(&mut self, text: &str) -> Result<()> {
        let kind = |c: Option<&str>, ab: &'static str, sq: &'static str| {
            if c == Some("<") { ab } else { sq }
        };
        let open = text.get(2..3);
        let token = if regex!(r"^\\g(.)[^0-9+\-].*['>]$").find(text).is_some() {
            kind(open, "name_call_ab", "name_call_sq")
        } else if regex!(r"^\\g(.)(?:0|0*[1-9]\d*)['>]$").find(text).is_some() {
            kind(open, "number_call_ab", "number_call_sq")
        } else if regex!(r"^\\g(.)[+-]0*[1-9]\d*").find(text).is_some() {
            kind(open, "number_rel_call_ab", "number_rel_call_sq")
        } else {
            return Err(ScanError);
        };
        self.emit("backref", token, text);
        Ok(())
    }

    // ---- groups ----------------------------------------------------------------

    fn group_open(&mut self, p: usize) -> Result<usize> {
        // Every pattern starting with `(` runs `group_opened` on it.
        self.group_depth += 1;
        if self.at(p + 1) != Some(b'?') {
            self.capturing_group_count += 1;
            self.emit("group", "capture", "(");
            return Ok(p + 1);
        }

        // `(?(`: conditional
        if self.at(p + 2) == Some(b'(') {
            self.conditional_stack.push(self.group_depth);
            self.emit("conditional", "open", "(?");
            self.emit("conditional", "condition_open", "(");
            return self.conditional_expression(p + 3);
        }

        let mut best: Option<(usize, GroupHead)> = None;
        let mut consider = |len: usize, head: GroupHead| {
            if best.is_none_or(|(l, _)| len > l) {
                best = Some((len, head));
            }
        };

        // (group_open . group_comment)
        if self.at(p + 2) == Some(b'#')
            && let Some(close) = self.b[p + 3..].iter().position(|&b| b == b')')
        {
            consider(p + 3 + close + 1 - p, GroupHead::Comment);
        }
        // (group_open . group_options)
        let opt_end = self.run(p + 2, |b| !b"!#'():<=>~".contains(&b));
        let opt_len = if opt_end > p + 2 {
            if self.at(opt_end) == Some(b':') {
                opt_end + 1 - p
            } else {
                opt_end - p
            }
        } else {
            2
        };
        consider(opt_len, GroupHead::Options);
        // (group_open . assertion_type)
        let rest = &self.b[p + 2..];
        if rest.starts_with(b"<=") || rest.starts_with(b"<!") {
            consider(4, GroupHead::Assertion);
        } else if rest.starts_with(b"=") || rest.starts_with(b"!") {
            consider(3, GroupHead::Assertion);
        }
        // (group_open . group_type)
        if rest.starts_with(b":") || rest.starts_with(b">") || rest.starts_with(b"~") {
            consider(3, GroupHead::Type);
        }
        if let Some(len) = self.group_name(p + 2) {
            consider(len + 2, GroupHead::Type);
        }

        let (len, head) = best.unwrap_or((2, GroupHead::Options));
        let text = self.copy(p, p + len);
        match head {
            GroupHead::Comment => {
                self.group_depth -= 1;
                self.emit("group", "comment", text);
            }
            GroupHead::Options => {
                if regex!(r"([^\-mixdau:]|^$)|-.*([dau])")
                    .find(&text[2..])
                    .is_some()
                {
                    return Err(ScanError);
                }
                self.emit_options(text);
            }
            GroupHead::Assertion => {
                let token = match text {
                    "(?=" => "lookahead",
                    "(?!" => "nlookahead",
                    "(?<=" => "lookbehind",
                    _ => "nlookbehind",
                };
                self.emit("assertion", token, text);
            }
            GroupHead::Type => match text {
                "(?:" => self.emit("group", "passive", text),
                "(?>" => self.emit("group", "atomic", text),
                "(?~" => self.emit("group", "absence", text),
                _ if text.starts_with("(?<>") || text.starts_with("(?''") => {
                    return Err(ScanError);
                }
                _ if text.starts_with("(?<") => self.emit("group", "named_ab", text),
                _ => self.emit("group", "named_sq", text),
            },
        }
        Ok(p + len)
    }

    /// `group_name` from `q` (`<name>` or `'name'`): its length.
    fn group_name(&self, q: usize) -> Option<usize> {
        let (close, excluded): (u8, &[u8]) = match self.at(q)? {
            b'<' => (b'>', b"!=0123456789->"),
            b'\'' => (b'\'', b"0123456789-'"),
            _ => return None,
        };
        let start = q + 1;
        let end = start + self.b[start..].iter().position(|&b| b == close)?;
        if end > start && excluded.contains(&self.b[start]) {
            return None;
        }
        Some(end + 1 - q)
    }

    fn emit_options(&mut self, text: &str) {
        let caps = regex!(r"\(\?([mixdau]*)(-(?:[mix]*))*(:)?").captures(text);
        let (positive, negative, group_local) = match &caps {
            Some(c) => (c.at(1).unwrap_or(""), c.at(2), c.at(3).is_some()),
            None => ("", None, false),
        };

        if positive.contains('x') {
            self.free_spacing = true;
        }
        // If the x appears in both, treat it like ruby does, the second
        // cancels the first.
        if negative.is_some_and(|n| n.contains('x')) {
            self.free_spacing = false;
        }

        let token = if group_local {
            self.spacing_stack.push(Spacing {
                free_spacing: self.free_spacing,
                depth: self.group_depth,
            });
            "options"
        } else {
            // switch for parent group level
            if let Some(last) = self.spacing_stack.last_mut() {
                last.free_spacing = self.free_spacing;
            }
            "options_switch"
        };

        self.emit("group", token, text);
    }

    fn group_close(&mut self, p: usize) -> Result<usize> {
        self.group_depth -= 1;
        if self.conditional_stack.last() == Some(&(self.group_depth + 1)) {
            self.conditional_stack.pop();
            self.emit("conditional", "close", ")");
        } else if self.group_depth >= 0 {
            if self.spacing_stack.len() > 1
                && self
                    .spacing_stack
                    .last()
                    .is_some_and(|s| s.depth == self.group_depth + 1)
            {
                self.spacing_stack.pop();
                self.free_spacing = self.spacing_stack.last().is_some_and(|s| s.free_spacing);
            }
            self.emit("group", "close", ")");
        } else {
            return Err(ScanError);
        }
        Ok(p + 1)
    }

    /// `conditional_expression`: a condition (`<name>)`, `'name')` or
    /// `1)`), then back to the main scanner.
    fn conditional_expression(&mut self, mut p: usize) -> Result<usize> {
        loop {
            let lookup_end = match self.at(p) {
                Some(b'<' | b'\'') => self.group_name(p).map(|len| p + len),
                Some(b'-' | b'0'..=b'9') => {
                    let start = if self.at(p) == Some(b'-') { p + 1 } else { p };
                    let end = self.run(start, |b| b.is_ascii_digit());
                    (end > start).then_some(end)
                }
                _ => None,
            };
            match lookup_end {
                Some(end) if self.at(end) == Some(b')') => {
                    let text = self.copy(p, end);
                    if !text.bytes().any(|b| b != b'0') {
                        return Err(ScanError);
                    }
                    self.emit("conditional", "condition", text);
                    self.emit("conditional", "condition_close", ")");
                    p = end + 1;
                }
                _ => return Ok(p),
            }
        }
    }

    // ---- character sets --------------------------------------------------------

    fn character_set(&mut self, mut p: usize) -> Result<usize> {
        loop {
            let Some(c) = self.at(p) else {
                return Err(ScanError);
            };
            match c {
                b']' => {
                    self.set_depth -= 1;
                    self.emit("set", "close", "]");
                    return Ok(p + 1);
                }
                b'-' if self.at(p + 1) == Some(b']') => {
                    self.set_depth -= 1;
                    self.emit("literal", "literal", "-");
                    self.emit("set", "close", "]");
                    return Ok(p + 2);
                }
                b'-' if self.b[p + 1..].starts_with(b"&&") => {
                    self.emit("literal", "literal", "-");
                    self.emit("set", "intersection", "&&");
                    p += 3;
                }
                b'^' => {
                    if self.prev_token.as_ref().is_some_and(|(_, t)| t == "open") {
                        self.emit("set", "negate", "^");
                    } else {
                        self.emit("literal", "literal", "^");
                    }
                    p += 1;
                }
                b'-' => {
                    // ranges cant start with the opening bracket, a subset, or
                    // intersection/negation/range operators
                    if self.prev_token.as_ref().is_some_and(|(ty, _)| *ty == "set") {
                        self.emit("literal", "literal", "-");
                    } else {
                        self.emit("set", "range", "-");
                    }
                    p += 1;
                }
                b'&' if self.at(p + 1) == Some(b'&') => {
                    self.emit("set", "intersection", "&&");
                    p += 2;
                }
                b'\\' => p = self.set_escape_sequence(p + 1)?,
                b'[' => {
                    if let Some(end) = self.class_posix(p) {
                        let text = self.copy(p, end);
                        let mut class_name = &text[2..text.len() - 2];
                        let mut ty = "posixclass";
                        if let Some(name) = class_name.strip_prefix('^') {
                            class_name = name;
                            ty = "nonposixclass";
                        }
                        if !POSIX_CLASSES.contains(&class_name) {
                            return Err(ScanError);
                        }
                        self.emit(ty, class_name, text);
                        p = end;
                    } else {
                        self.set_depth += 1;
                        self.emit("set", "open", "[");
                        p = self.character_set(p + 1)?;
                    }
                }
                _ => {
                    let end = p + self.char_len(p);
                    let text = self.copy(p, end);
                    self.emit("literal", "literal", text);
                    p = end;
                }
            }
        }
    }

    /// `class_posix`: `[:` `^`? `[^\[\]]*` `:]`.
    fn class_posix(&self, p: usize) -> Option<usize> {
        if self.at(p + 1) != Some(b':') {
            return None;
        }
        let j = p
            + 2
            + self.b[p + 2..]
                .iter()
                .position(|&b| b == b'[' || b == b']')?;
        (self.b[j] == b']' && j >= p + 3 && self.b[j - 1] == b':').then_some(j + 1)
    }

    /// `set_escape_sequence`, after the backslash at `p - 1`.
    fn set_escape_sequence(&mut self, p: usize) -> Result<usize> {
        let Some(c) = self.at(p) else {
            return Err(ScanError);
        };
        if (b'0'..=b'7').contains(&c) {
            let end = self.octal_end(p);
            let text = self.copy(p - 1, end);
            self.emit("escape", "octal", text);
            return Ok(end);
        }
        let set_escape = matches!(
            c,
            b'\\'
                | b'['
                | b']'
                | b'a'
                | b'b'
                | b'e'
                | b'f'
                | b'n'
                | b'r'
                | b't'
                | b'v'
                | b'p'
                | b'P'
                | b'C'
                | b'M'
                | b'c'
                | b'u'
                | b'x'
                | b'd'
                | b'D'
                | b'h'
                | b'H'
                | b's'
                | b'S'
                | b'w'
                | b'W'
        );
        if set_escape {
            return self.escape_sequence(p);
        }
        let end = p + self.char_len(p);
        let text = self.copy(p - 1, end);
        self.emit("escape", "literal", text);
        Ok(end)
    }

    fn octal_end(&self, p: usize) -> usize {
        let mut end = p;
        while end < p + 3 && self.at(end).is_some_and(|b| (b'0'..=b'7').contains(&b)) {
            end += 1;
        }
        end
    }

    // ---- escapes -------------------------------------------------------------

    /// `escape_sequence`, after the backslash at `p - 1`.
    fn escape_sequence(&mut self, p: usize) -> Result<usize> {
        let Some(c) = self.at(p) else {
            return Err(ScanError);
        };
        let ts = p - 1;
        match c {
            b'1'..=b'9' => {
                let end = self.run(p, |b| b.is_ascii_digit());
                let text = self.copy(ts, end);
                // If not enough groups have been opened, there is a fallback
                // to either an octal or literal interpretation for 2+ digit
                // numerical escapes.
                let digits = &text[1..];
                let value: i64 = digits.parse().unwrap_or(i64::MAX);
                if digits.len() == 1 || value <= self.capturing_group_count {
                    self.emit("backref", "number", text);
                } else if digits.len() >= 2 && digits.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                    self.emit("escape", "octal", text);
                } else {
                    self.emit("escape", "literal", &text[0..2]);
                    self.emit("literal", "literal", &text[2..]);
                }
                Ok(end)
            }
            b'0' => {
                let end = self.octal_end(p);
                let text = self.copy(ts, end);
                self.emit("escape", "octal", text);
                Ok(end)
            }
            c if is_meta_char(c) => {
                let text = self.copy(ts, p + 1);
                let token = match c {
                    b'.' => "dot",
                    b'|' => "alternation",
                    b'^' => "bol",
                    b'$' => "eol",
                    b'?' => "zero_or_one",
                    b'*' => "zero_or_more",
                    b'+' => "one_or_more",
                    b'(' => "group_open",
                    b')' => "group_close",
                    b'{' => "interval_open",
                    b'}' => "interval_close",
                    b'[' => "set_open",
                    b']' => "set_close",
                    _ => "backslash",
                };
                self.emit("escape", token, text);
                Ok(p + 1)
            }
            b'a' | b'b' | b'e' | b'f' | b'n' | b'r' | b't' | b'v' => {
                let text = self.copy(ts, p + 1);
                let token = match c {
                    b'a' => "bell",
                    b'b' => "backspace",
                    b'e' => "escape",
                    b'f' => "form_feed",
                    b'n' => "newline",
                    b'r' => "carriage",
                    b't' => "tab",
                    _ => "vertical_tab",
                };
                self.emit("escape", token, text);
                Ok(p + 1)
            }
            b'u' => {
                let end = if self.at(p + 1) == Some(b'{') {
                    self.codepoint_list_end(p + 2)?
                } else {
                    let end = self.run(p + 1, is_xdigit).min(p + 5);
                    if end != p + 5 {
                        return Err(ScanError);
                    }
                    end
                };
                let text = self.copy(ts, end);
                let token = if text.as_bytes()[2] == b'{' {
                    "codepoint_list"
                } else {
                    "codepoint"
                };
                self.emit("escape", token, text);
                Ok(end)
            }
            b'x' => self.hex_escape(p),
            b'c' | b'C' => {
                let q = if c == b'c' {
                    p + 1
                } else if self.at(p + 1) == Some(b'-') {
                    p + 2
                } else {
                    return Err(ScanError);
                };
                let q = if self.b[q..].starts_with(b"\\M-") {
                    q + 3
                } else {
                    q
                };
                let end = self.escaped_any_end(q)?;
                self.emit_meta_control_sequence(ts, end, "control")?;
                Ok(end)
            }
            b'M' => {
                if self.at(p + 1) != Some(b'-') {
                    return Err(ScanError);
                }
                let mut q = p + 2;
                if self.b[q..].starts_with(b"\\c") {
                    q += 2;
                } else if self.b[q..].starts_with(b"\\C-") {
                    q += 3;
                }
                let end = self.escaped_any_end(q)?;
                self.emit_meta_control_sequence(ts, end, "meta_sequence")?;
                Ok(end)
            }
            b'd' | b'D' | b'h' | b'H' | b's' | b'S' | b'w' | b'W' | b'R' | b'X' => {
                let text = self.copy(ts, p + 1);
                let token = match c {
                    b'd' => "digit",
                    b'D' => "nondigit",
                    b'h' => "hex",
                    b'H' => "nonhex",
                    b's' => "space",
                    b'S' => "nonspace",
                    b'w' => "word",
                    b'W' => "nonword",
                    b'R' => "linebreak",
                    _ => "xgrapheme",
                };
                self.emit("type", token, text);
                Ok(p + 1)
            }
            b'p' | b'P' => self.unicode_property(p),
            // non_literal_escape characters that matched nothing above
            b'A' | b'B' | b'z' | b'Z' | b'G' | b'K' => Err(ScanError),
            _ => {
                let end = p + self.char_len(p);
                let text = self.copy(ts, end);
                self.emit("escape", "literal", text);
                Ok(end)
            }
        }
    }

    /// `'\\'? . any` from `q`: its end.
    fn escaped_any_end(&self, q: usize) -> Result<usize> {
        match self.at(q) {
            None => Err(ScanError),
            Some(b'\\') if q + 1 < self.b.len() => Ok(q + 1 + self.char_len(q + 1)),
            Some(_) => Ok(q + self.char_len(q)),
        }
    }

    fn emit_meta_control_sequence(&mut self, ts: usize, te: usize, token: &str) -> Result<()> {
        // `data.last`: the last byte of the whole pattern.
        if self.b.last().is_some_and(|&b| b > 0x7f) {
            return Err(ScanError);
        }
        let text = self.copy(ts, te);
        self.emit("escape", token, text);
        Ok(())
    }

    /// `'u{' . xdigit{1,6} . (space . xdigit{1,6})* . '}'` from after `u{`.
    fn codepoint_list_end(&self, q: usize) -> Result<usize> {
        let mut q = q;
        loop {
            let end = self.run(q, is_xdigit);
            if end == q || end - q > 6 {
                return Err(ScanError);
            }
            match self.at(end) {
                Some(b'}') => return Ok(end + 1),
                Some(b) if is_space(b) => q = end + 1,
                _ => return Err(ScanError),
            }
        }
    }

    fn hex_escape(&mut self, p: usize) -> Result<usize> {
        let ts = p - 1;
        let high = |q: usize| {
            self.at(q)
                .is_some_and(|b| matches!(b, b'8' | b'9' | b'A'..=b'F' | b'a'..=b'f'))
                && self.at(q + 1).is_some_and(is_xdigit)
        };
        if high(p + 1) {
            // high_hex_sequence
            let mut end = p + 3;
            while self.b[end..].starts_with(b"\\x") && high(end + 2) {
                end += 4;
            }
            let text = self.copy(ts, end);
            if self.binary {
                let mut start = 0;
                for (i, _) in text.match_indices('\\').skip(1) {
                    self.emit("escape", "hex", &text[start..i]);
                    start = i;
                }
                self.emit("escape", "hex", &text[start..]);
            } else {
                self.emit("escape", "utf8_hex", text);
            }
            return Ok(end);
        }
        match self.at(p + 1) {
            Some(b) if is_xdigit(b) => {
                let end = if self.at(p + 2).is_some_and(is_xdigit) {
                    p + 3
                } else {
                    p + 2
                };
                let text = self.copy(ts, end);
                self.emit("escape", "hex", text);
                Ok(end)
            }
            _ => Err(ScanError),
        }
    }

    /// `unicode_property`, at the `p`/`P` at `p`.
    fn unicode_property(&mut self, p: usize) -> Result<usize> {
        let ts = p - 1;
        if self.at(p + 1) == Some(b'{') {
            let mut q = p + 2;
            if self.at(q) == Some(b'^') {
                q += 1;
            }
            let start = q;
            while self
                .at(q)
                .is_some_and(|b| b.is_ascii_alphanumeric() || is_space(b) || b"_-.=".contains(&b))
            {
                q += 1;
            }
            match self.at(q) {
                None => return Err(ScanError),
                Some(b'}') if q > start => {
                    let end = q + 1;
                    let text = self.copy(ts, end);
                    let bytes = text.as_bytes();
                    let ty = if (bytes[1] == b'P') ^ (bytes[3] == b'^') {
                        "nonproperty"
                    } else {
                        "property"
                    };
                    let name: String = text[3..text.len() - 1]
                        .chars()
                        .filter(|c| !matches!(c, '^' | '_' | '-') && !c.is_whitespace())
                        .collect::<String>()
                        .to_lowercase();
                    let Some(token) = SHORT_PROP_MAP
                        .get(name.as_str())
                        .or_else(|| LONG_PROP_MAP.get(name.as_str()))
                    else {
                        return Err(ScanError);
                    };
                    self.emit(ty, token, text);
                    return Ok(end);
                }
                _ => {}
            }
        }
        // a lone "\p" without a curly bracket after it
        let text = self.copy(ts, p + 1);
        self.emit("escape", "literal", text);
        Ok(p + 1)
    }
}

#[derive(Debug, Clone, Copy)]
enum GroupHead {
    Comment,
    Options,
    Assertion,
    Type,
}
