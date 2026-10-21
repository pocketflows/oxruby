//! Port of `regexp_parser`'s `Regexp::Expression` classes
//! (`lib/regexp_parser/expression/**`): the parse tree, in an arena.

/// The expression classes, as far as their behavior differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Root,
    /// `Sequence` and its subclasses `Alternative`, `Branch` and
    /// `CharacterSet::IntersectedSequence`.
    Sequence(SequenceKind),
    /// `Alternation`
    Alternation,
    /// `CharacterSet::Intersection`
    Intersection,
    CharacterSet,
    /// `CharacterSet::Range`
    Range,
    Group(GroupKind),
    Assertion,
    /// `Conditional::Expression`
    Conditional,
    /// `Conditional::Condition`
    Condition,
    Backreference(BackrefKind),
    /// `FreeSpace`: `Comment` and `WhiteSpace`.
    FreeSpace,
    /// Any other terminal expression (literals, escapes, anchors, types,
    /// properties, posix classes, keep marks).
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind {
    Alternative,
    Branch,
    IntersectedSequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Passive,
    Absence,
    Atomic,
    Options,
    Capture,
    Named,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackrefKind {
    Number,
    Name,
    NumberRelative,
    NumberCall,
    NameCall,
    NumberCallRelative,
    NumberRecursionLevel,
    NameRecursionLevel,
}

/// `Regexp::Expression::Quantifier`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantifier {
    /// The token without the mode (`zero_or_one`, `interval`, ...).
    pub token: String,
    pub text: String,
    pub ts: usize,
    pub min: i64,
    /// `-1` for no maximum.
    pub max: i64,
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Greedy,
    Reluctant,
    Possessive,
}

impl Quantifier {
    pub fn new(token: &str, text: &str, ts: usize) -> Self {
        let token = token
            .replacen("_greedy", "", 1)
            .replacen("_possessive", "", 1)
            .replacen("_reluctant", "", 1);
        let (min, max) = match text.as_bytes().first() {
            Some(b'?') => (0, 1),
            Some(b'*') => (0, -1),
            Some(b'+') => (1, -1),
            _ => {
                let inner = text.trim_start_matches('{');
                let int_min: String = inner.chars().take_while(char::is_ascii_digit).collect();
                // `text[/,?(\d*)\}/, 1]`
                let before_close = &text[..text.rfind('}').unwrap_or(text.len())];
                let int_max: String = before_close
                    .chars()
                    .rev()
                    .take_while(char::is_ascii_digit)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                (
                    int_min.parse().unwrap_or(0),
                    if int_max.is_empty() {
                        -1
                    } else {
                        int_max.parse().unwrap_or(-1)
                    },
                )
            }
        };
        let modifier = text.chars().nth(1);
        let mode = match modifier {
            Some('?') => Mode::Reluctant,
            Some('+') => Mode::Possessive,
            _ => Mode::Greedy,
        };
        Self {
            token,
            text: text.to_owned(),
            ts,
            min,
            max,
            mode,
        }
    }

    pub fn is_greedy(&self) -> bool {
        self.mode == Mode::Greedy
    }
}

pub type ExprId = usize;

/// One expression (`Regexp::Expression::Base`).
#[derive(Debug, Clone)]
pub struct Expression {
    pub class: Class,
    pub ty: String,
    pub token: String,
    pub text: String,
    /// The token's start; for sequences, ranges and sequence operations
    /// `ts` falls back to it when they are empty.
    pub own_ts: usize,
    pub level: i64,
    pub set_level: i64,
    pub conditional_level: i64,
    pub parent: Option<ExprId>,
    pub quantifier: Option<Quantifier>,
    pub expressions: Vec<ExprId>,
    pub negative: bool,
    pub closed: bool,
    pub implicit: bool,
    pub custom_to_s_handling: bool,
    pub pre_quantifier_decorations: Vec<String>,
    /// Capture group number (`Group::Capture#number`).
    pub number: Option<i64>,
    /// `Group::Named#name`, `Backreference::Name#name`.
    pub name: Option<String>,
    /// `Backreference::NumberRelative#effective_number`.
    pub effective_number: Option<i64>,
}

impl Expression {
    pub fn new(class: Class, ty: &str, token: &str, text: &str, ts: usize) -> Self {
        Self {
            class,
            ty: ty.to_owned(),
            token: token.to_owned(),
            text: text.to_owned(),
            own_ts: ts,
            level: 0,
            set_level: 0,
            conditional_level: 0,
            parent: None,
            quantifier: None,
            expressions: Vec::new(),
            negative: false,
            closed: false,
            implicit: false,
            custom_to_s_handling: false,
            pre_quantifier_decorations: Vec::new(),
            number: None,
            name: None,
            effective_number: None,
        }
    }

    /// `terminal?`
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.class,
            Class::Condition | Class::Backreference(_) | Class::FreeSpace | Class::Terminal
        )
    }

    /// `decorative?`
    pub fn is_decorative(&self) -> bool {
        matches!(
            self.class,
            Class::FreeSpace | Class::Group(GroupKind::Comment)
        )
    }

    /// `capturing?`
    pub fn is_capturing(&self) -> bool {
        matches!(
            self.class,
            Class::Group(GroupKind::Capture | GroupKind::Named)
        )
    }

    /// `referential?`
    pub fn is_referential(&self) -> bool {
        matches!(
            self.class,
            Class::Backreference(_) | Class::Condition | Class::Conditional
        )
    }

    /// `is?(token, type)`
    pub fn is(&self, token: &str, ty: Option<&str>) -> bool {
        self.token == token && ty.is_none_or(|t| self.ty == t)
    }

    /// `type?(type)`
    pub fn is_type(&self, ty: &str) -> bool {
        self.ty == ty
    }

    pub fn is_quantified(&self) -> bool {
        self.quantifier.is_some()
    }

    /// `greedy?`
    pub fn is_greedy(&self) -> bool {
        self.quantifier.as_ref().is_some_and(Quantifier::is_greedy)
    }

    /// `quantity`
    pub fn quantity(&self) -> Option<(i64, i64)> {
        self.quantifier.as_ref().map(|q| (q.min, q.max))
    }
}

/// A parse tree: the root is expression 0.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub exprs: Vec<Expression>,
}

enum Part {
    Str(String),
    Expr(ExprId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Full,
    Original,
    Base,
}

/// The events of `traverse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Enter,
    Visit,
    Exit,
}

impl Tree {
    pub const ROOT: ExprId = 0;

    pub fn get(&self, id: ExprId) -> &Expression {
        &self.exprs[id]
    }

    pub fn root(&self) -> &Expression {
        &self.exprs[Self::ROOT]
    }

    /// `ts`: sequences, ranges and sequence operations start at their first
    /// expression.
    pub fn ts(&self, id: ExprId) -> usize {
        let exp = self.get(id);
        match exp.class {
            Class::Sequence(_) | Class::Alternation | Class::Intersection | Class::Range => {
                match exp.expressions.first() {
                    Some(&head) => self.ts(head),
                    None => exp.own_ts,
                }
            }
            // Root is a plain Subexpression with `ts: 0`; implicit sequences
            // built by `construct` default to their given `ts`.
            _ => exp.own_ts,
        }
    }

    /// `te`
    pub fn te(&self, id: ExprId) -> usize {
        self.ts(id) + self.base_length(id)
    }

    pub fn base_length(&self, id: ExprId) -> usize {
        self.to_s(id, Format::Base).chars().count()
    }

    pub fn full_length(&self, id: ExprId) -> usize {
        self.to_s(id, Format::Original).chars().count()
    }

    fn intersperse(&self, expressions: &[ExprId], separator: &str) -> Vec<Part> {
        let mut parts = Vec::new();
        for (i, &e) in expressions.iter().enumerate() {
            if i > 0 {
                parts.push(Part::Str(separator.to_owned()));
            }
            parts.push(Part::Expr(e));
        }
        parts
    }

    /// `parts`
    fn parts(&self, id: ExprId) -> Vec<Part> {
        let exp = self.get(id);
        let exprs = |list: &[ExprId]| list.iter().map(|&e| Part::Expr(e)).collect::<Vec<_>>();
        match exp.class {
            Class::CharacterSet => {
                let mut parts = vec![Part::Str(format!(
                    "{}{}",
                    exp.text,
                    if exp.negative { "^" } else { "" }
                ))];
                parts.extend(exprs(&exp.expressions));
                parts.push(Part::Str("]".to_owned()));
                parts
            }
            Class::Range | Class::Alternation | Class::Intersection => {
                self.intersperse(&exp.expressions, &exp.text)
            }
            Class::Conditional => {
                let mut parts = vec![Part::Str(exp.text.clone())];
                let condition = exp
                    .expressions
                    .iter()
                    .copied()
                    .find(|&e| self.get(e).class == Class::Condition);
                if let Some(condition) = condition {
                    parts.push(Part::Expr(condition));
                }
                let branches: Vec<ExprId> = exp
                    .expressions
                    .iter()
                    .copied()
                    .filter(|&e| matches!(self.get(e).class, Class::Sequence(_)))
                    .collect();
                parts.extend(self.intersperse(&branches, "|"));
                parts.push(Part::Str(")".to_owned()));
                parts
            }
            Class::Group(GroupKind::Comment) => vec![Part::Str(exp.text.clone())],
            Class::Group(GroupKind::Passive) if exp.implicit => exprs(&exp.expressions),
            Class::Group(_) | Class::Assertion => {
                let mut parts = vec![Part::Str(exp.text.clone())];
                parts.extend(exprs(&exp.expressions));
                parts.push(Part::Str(")".to_owned()));
                parts
            }
            Class::Root | Class::Sequence(_) => exprs(&exp.expressions),
            _ => vec![Part::Str(exp.text.clone())],
        }
    }

    /// `to_s(format)`
    pub fn to_s(&self, id: ExprId, format: Format) -> String {
        let mut result = String::new();
        self.to_s_into(id, format, &mut result);
        result
    }

    fn to_s_into(&self, id: ExprId, format: Format, result: &mut String) {
        for part in self.parts(id) {
            match part {
                Part::Str(s) => result.push_str(&s),
                Part::Expr(e) => {
                    if !self.get(e).custom_to_s_handling {
                        self.to_s_into(e, Format::Original, result);
                    }
                }
            }
        }
        let exp = self.get(id);
        if format == Format::Original {
            for decoration in &exp.pre_quantifier_decorations {
                result.push_str(decoration);
            }
        }
        if format != Format::Base
            && let Some(q) = &exp.quantifier
        {
            result.push_str(&q.text);
        }
    }

    /// `each_expression(include_self)`: pre-order.
    pub fn each_expression(&self, id: ExprId, include_self: bool) -> Vec<ExprId> {
        let mut out = Vec::new();
        if include_self {
            out.push(id);
        }
        let mut queue: Vec<ExprId> = self.get(id).expressions.iter().rev().copied().collect();
        while let Some(exp) = queue.pop() {
            out.push(exp);
            if !self.get(exp).is_terminal() {
                queue.extend(self.get(exp).expressions.iter().rev().copied());
            }
        }
        out
    }

    /// `traverse(include_self)`: `(event, expression)` pairs.
    pub fn traverse(&self, id: ExprId, include_self: bool) -> Vec<(Event, ExprId)> {
        let mut out = Vec::new();
        if include_self {
            out.push((Event::Enter, id));
        }
        self.traverse_children(id, &mut out);
        if include_self {
            out.push((Event::Exit, id));
        }
        out
    }

    fn traverse_children(&self, id: ExprId, out: &mut Vec<(Event, ExprId)>) {
        for &exp in &self.get(id).expressions {
            if self.get(exp).is_terminal() {
                out.push((Event::Visit, exp));
            } else {
                out.push((Event::Enter, exp));
                self.traverse_children(exp, out);
                out.push((Event::Exit, exp));
            }
        }
    }
}
