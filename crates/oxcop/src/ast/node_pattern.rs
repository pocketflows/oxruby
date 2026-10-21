//! Port of rubocop-ast's `NodePattern` (lexer, parser and the semantics of
//! its compiler) as an interpreter, so cops can use their `def_node_matcher`
//! patterns verbatim:
//!
//! ```ignore
//! static PATTERN: NodePattern = NodePattern::new("(send nil? :puts $_)");
//! if let Some(captures) = PATTERN.match_node(node) { ... }
//! ```
//!
//! `#method` calls and constants are resolved through [`MatchContext`].
//! Sequences match like the compiled Ruby code does: variadic terms are
//! greedy (with backtracking, a superset of Ruby's matches).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ast::{Child, Node, NodeType};

// ---- values ------------------------------------------------------------------

/// What a pattern element is matched against: a node, or a non-node child
/// value (`nil`, a symbol, a number, ...), or a node's type at the head of a
/// sequence.
#[derive(Debug, Clone, Copy)]
pub enum Val<'a> {
    Node(Node<'a>),
    Value(&'a Child),
    Type(NodeType),
    /// Literal arguments (`#foo(:bar)`) and parameter values.
    Sym(&'a str),
    Str(&'a str),
    Int(i64),
}

impl<'a> Val<'a> {
    pub fn node(&self) -> Option<Node<'a>> {
        match self {
            Val::Node(n) => Some(*n),
            _ => None,
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Val::Value(Child::Nil))
    }

    pub fn sym(&self) -> Option<&'a str> {
        match self {
            Val::Value(Child::Sym(s)) => Some(s),
            Val::Type(t) => Some(t.name()),
            Val::Sym(s) => Some(s),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&'a str> {
        match self {
            Val::Value(Child::Str(s)) => Some(s),
            Val::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Val::Value(Child::Int(i)) => Some(*i),
            Val::Int(i) => Some(*i),
            _ => None,
        }
    }

    fn from_child(child: &'a Child, node: Node<'a>) -> Val<'a> {
        match child {
            Child::Node(id) => Val::Node(node.ast().node(*id)),
            other => Val::Value(other),
        }
    }

    /// Ruby `==` between two values (structural for nodes).
    fn equals(&self, other: &Val<'_>) -> bool {
        match (self, other) {
            (Val::Node(a), Val::Node(b)) => a.same_as(*b),
            (Val::Value(a), Val::Value(b)) => a == b,
            (Val::Node(_), _) | (_, Val::Node(_)) => false,
            _ => {
                if let (Some(a), Some(b)) = (self.sym(), other.sym()) {
                    a == b
                } else if let (Some(a), Some(b)) = (self.str(), other.str()) {
                    a == b
                } else if let (Some(a), Some(b)) = (self.int(), other.int()) {
                    a == b
                } else {
                    self.is_nil() && other.is_nil()
                }
            }
        }
    }
}

/// A captured value: one element, or a list (`$...`, repetitions).
#[derive(Debug, Clone)]
pub enum Capture<'a> {
    One(Val<'a>),
    Many(Vec<Capture<'a>>),
}

impl<'a> Capture<'a> {
    pub fn node(&self) -> Option<Node<'a>> {
        match self {
            Capture::One(v) => v.node(),
            Capture::Many(_) => None,
        }
    }

    pub fn val(&self) -> Option<Val<'a>> {
        match self {
            Capture::One(v) => Some(*v),
            Capture::Many(_) => None,
        }
    }

    pub fn sym(&self) -> Option<&'a str> {
        self.val().and_then(|v| v.sym())
    }

    pub fn str(&self) -> Option<&'a str> {
        self.val().and_then(|v| v.str())
    }

    pub fn is_nil(&self) -> bool {
        self.val().is_some_and(|v| v.is_nil())
    }

    pub fn list(&self) -> &[Capture<'a>] {
        match self {
            Capture::Many(list) => list,
            Capture::One(_) => std::slice::from_ref(self),
        }
    }

    /// The nodes of a list capture.
    pub fn nodes(&self) -> Vec<Node<'a>> {
        self.list().iter().filter_map(Capture::node).collect()
    }
}

/// A parameter (`%1`, `%name`) or constant (`CONST`) value supplied by the
/// cop, compared with Ruby's `===`.
#[derive(Clone, Copy)]
pub enum Param<'p> {
    Sym(&'p str),
    /// A `Set` of symbols.
    Syms(&'p [&'p str]),
    /// A `Set` of strings.
    Strs(&'p [&'p str]),
    Str(&'p str),
    Int(i64),
    Bool(bool),
    Nil,
    /// A node (`===` is structural equality).
    Node(Node<'p>),
    /// A regexp, matched against strings and symbols.
    Regex(&'p onig::Regex),
    /// Any other `===`.
    Fn(&'p dyn Fn(&Val<'_>) -> bool),
}

impl Param<'_> {
    fn case_eq(&self, val: &Val<'_>) -> bool {
        match self {
            Param::Sym(s) => val.sym() == Some(s),
            Param::Syms(set) => val.sym().is_some_and(|v| set.contains(&v)),
            Param::Strs(set) => val.str().is_some_and(|v| set.contains(&v)),
            Param::Str(s) => val.str() == Some(s),
            Param::Int(i) => val.int() == Some(*i),
            // `true === x` is `x == true`; children are never booleans.
            Param::Bool(_) => false,
            Param::Nil => val.is_nil(),
            Param::Node(n) => val.node().is_some_and(|v| v.same_as(*n)),
            Param::Regex(re) => match val.sym().or(val.str()) {
                Some(s) => re.find(s).is_some(),
                None => false,
            },
            Param::Fn(f) => f(val),
        }
    }
}

/// Function calls (`#method`) and parameter/constant lookup for a match.
#[derive(Default)]
pub struct MatchContext<'c, 'p> {
    /// `%1`, `%2`, ... (`%1` is index 0).
    pub params: &'c [Param<'p>],
    /// `%name` and `CONST`.
    pub named: &'c [(&'c str, Param<'p>)],
    /// `#method(element, args...)`.
    #[allow(clippy::type_complexity)]
    pub call: Option<&'c dyn Fn(&str, &Val<'_>, &[Val<'_>]) -> bool>,
}

// ---- pattern AST -------------------------------------------------------------

#[derive(Debug, Clone)]
enum Pat {
    Wildcard,
    Rest,
    NodeType(String),
    Sym(String),
    Int(i64),
    Float(f64),
    Str(String),
    Regex(String, String),
    /// `CONST` / `%CONST` and `%name`: looked up in `named`.
    Named(String),
    /// `%1` is `Positional(1)`.
    Positional(usize),
    Unify(String),
    Capture(Box<Pat>),
    Negation(Box<Pat>),
    Ascend(Box<Pat>),
    Descend(Box<Pat>),
    Sequence(Vec<Pat>),
    Intersection(Vec<Pat>),
    /// Each branch is a subsequence (one element for simple unions).
    Union(Vec<Vec<Pat>>),
    Predicate(String, Option<Vec<Pat>>),
    Function(String, Option<Vec<Pat>>),
    Repetition(Box<Pat>, char),
    AnyOrder(Vec<Pat>, Option<Box<Pat>>),
}

const INF: usize = usize::MAX;

impl Pat {
    /// `arity_range` within a sequence.
    fn arity(&self) -> (usize, usize) {
        match self {
            Pat::Rest => (0, INF),
            Pat::Repetition(_, '?') => (0, 1),
            Pat::Repetition(_, '*') => (0, INF),
            Pat::Repetition(_, _) => (1, INF),
            Pat::Capture(child) => child.arity(),
            Pat::AnyOrder(terms, rest) => {
                let n = terms.len();
                (n, if rest.is_some() { INF } else { n })
            }
            Pat::Union(branches) => {
                let mut min = INF;
                let mut max = 0;
                for branch in branches {
                    let (bmin, bmax) = seq_arity(branch);
                    min = min.min(bmin);
                    max = max.max(bmax);
                }
                (min, max)
            }
            _ => (1, 1),
        }
    }

    fn is_variadic(&self) -> bool {
        self.arity() != (1, 1)
    }
}

fn seq_arity(terms: &[Pat]) -> (usize, usize) {
    terms.iter().fold((0, 0), |(min, max), t| {
        let (tmin, tmax) = t.arity();
        (
            min + tmin,
            if max == INF || tmax == INF {
                INF
            } else {
                max + tmax
            },
        )
    })
}

// ---- lexer ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Symbol(String),
    Str(String),
    Int(i64),
    Float(f64),
    Punct(&'static str),
    Regexp(String, String),
    ParamConst(String),
    ParamNamed(String),
    ParamNumber(usize),
    Unify(String),
    Wildcard,
    FunctionCall(String),
    Predicate(String),
    NodeType(String),
    ArgList,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_lowercase()
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn lex(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut arg_state = false;
    while i < b.len() {
        if arg_state {
            arg_state = false;
            if b[i] == b'(' {
                out.push(Tok::ArgList);
                i += 1;
                continue;
            }
        }
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // :symbol
        if c == b':' {
            let rest = &src[i + 1..];
            let len = if rest.starts_with("[]=") {
                3
            } else if rest.starts_with("[]") {
                2
            } else {
                rest.bytes()
                    .take_while(|&c| is_ident(c) || b"+@*/?!<>=~|%^&-".contains(&c))
                    .count()
            };
            if len > 0 {
                out.push(Tok::Symbol(rest[..len].to_owned()));
                i += 1 + len;
                continue;
            }
        }
        // "string"
        if c == b'"' {
            let end = src[i + 1..]
                .find('"')
                .expect("unterminated string in node pattern");
            out.push(Tok::Str(src[i + 1..i + 1 + end].to_owned()));
            i += end + 2;
            continue;
        }
        // numbers
        if c.is_ascii_digit()
            || ((c == b'-' || c == b'+') && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                out.push(Tok::Float(src[start..i].parse().unwrap()));
            } else {
                out.push(Tok::Int(src[start..i].parse().unwrap()));
            }
            continue;
        }
        if src[i..].starts_with("...") {
            out.push(Tok::Punct("..."));
            i += 3;
            continue;
        }
        if let Some(p) = [
            "(", ")", "{", "|", "}", "[", "]", "<", ">", "$", "!", "^", "`", "+", "*", "?", ",",
        ]
        .iter()
        .find(|p| src[i..].starts_with(**p))
        {
            out.push(Tok::Punct(p));
            i += 1;
            continue;
        }
        // regexp
        if c == b'/' {
            let mut j = i + 1;
            while j < b.len() && !(b[j] == b'/' && b[j - 1] != b'\\') {
                j += 1;
            }
            let body = src[i + 1..j].to_owned();
            j += 1;
            let opts_start = j;
            while j < b.len() && b"imxo".contains(&b[j]) {
                j += 1;
            }
            out.push(Tok::Regexp(body, src[opts_start..j].to_owned()));
            i = j;
            continue;
        }
        // %CONST / CONST
        let const_at = |k: usize| -> usize {
            if k < b.len() && (b[k].is_ascii_uppercase() || b[k] == b':') {
                let len = b[k..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphabetic() || **c == b'_' || **c == b':')
                    .count();
                if len >= 2 { len } else { 0 }
            } else {
                0
            }
        };
        if c == b'%' {
            let len = const_at(i + 1);
            if len > 0 {
                out.push(Tok::ParamConst(src[i + 1..i + 1 + len].to_owned()));
                i += 1 + len;
                continue;
            }
            let len = b[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_lowercase() || **c == b'_')
                .count();
            if len > 0 {
                out.push(Tok::ParamNamed(src[i + 1..i + 1 + len].to_owned()));
                i += 1 + len;
                continue;
            }
            let len = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
            let n = if len == 0 {
                1
            } else {
                src[i + 1..i + 1 + len].parse().unwrap()
            };
            out.push(Tok::ParamNumber(n));
            i += 1 + len;
            continue;
        }
        let len = const_at(i);
        if len > 0 {
            out.push(Tok::ParamConst(src[i..i + len].to_owned()));
            i += len;
            continue;
        }
        // _name / _
        if c == b'_' {
            if b.get(i + 1).copied().is_some_and(is_ident_start) {
                let len = b[i + 1..].iter().take_while(|c| is_ident(**c)).count();
                out.push(Tok::Unify(src[i + 1..i + 1 + len].to_owned()));
                i += 1 + len;
            } else {
                out.push(Tok::Wildcard);
                i += 1;
            }
            continue;
        }
        // #call
        if c == b'#' {
            let rest = &b[i + 1..];
            let mut len = 0;
            // (?:CONST_NAME\.)?IDENTIFIER[!?]?
            let clen = const_at(i + 1);
            if clen > 0 && rest.get(clen) == Some(&b'.') {
                len = clen + 1;
            }
            if rest.get(len).copied().is_some_and(is_ident_start) {
                len += rest[len..].iter().take_while(|c| is_ident(**c)).count();
                if rest.get(len).is_some_and(|c| *c == b'!' || *c == b'?') {
                    len += 1;
                }
                out.push(Tok::FunctionCall(src[i + 1..i + 1 + len].to_owned()));
                i += 1 + len;
                arg_state = true;
                continue;
            }
            // A comment: skip to the end of the line.
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if is_ident_start(c) {
            let len = b[i..].iter().take_while(|c| is_ident(**c)).count();
            if b.get(i + len) == Some(&b'?') {
                out.push(Tok::Predicate(src[i..i + len + 1].to_owned()));
                i += len + 1;
                arg_state = true;
                continue;
            }
            let len = b[i..]
                .iter()
                .take_while(|c| is_ident(**c) || **c == b'-')
                .count();
            out.push(Tok::NodeType(src[i..i + len].to_owned()));
            i += len;
            continue;
        }
        panic!("node pattern: unexpected {:?} in {src:?}", &src[i..]);
    }
    out
}

// ---- parser ----------------------------------------------------------------------

struct Parser<'s> {
    tokens: Vec<Tok>,
    pos: usize,
    src: &'s str,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Tok {
        let t = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| panic!("node pattern: unexpected end of {:?}", self.src));
        self.pos += 1;
        t
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn expect(&mut self, p: &str) {
        match self.next() {
            Tok::Punct(q) if q == p => {}
            other => panic!(
                "node pattern: expected {p:?}, got {other:?} in {:?}",
                self.src
            ),
        }
    }

    /// `node_pattern`: a single (non-variadic) pattern.
    fn node_pattern(&mut self) -> Pat {
        if self.is_punct("{") {
            return self.union();
        }
        self.node_pattern_no_union()
    }

    fn node_pattern_no_union(&mut self) -> Pat {
        match self.next() {
            Tok::Punct("(") => {
                let mut terms = Vec::new();
                while !self.is_punct(")") {
                    terms.push(self.variadic_pattern());
                }
                self.expect(")");
                Pat::Sequence(terms)
            }
            Tok::Punct("[") => {
                let mut terms = Vec::new();
                while !self.is_punct("]") {
                    terms.push(self.node_pattern());
                }
                self.expect("]");
                Pat::Intersection(terms)
            }
            Tok::Punct("!") => Pat::Negation(Box::new(self.node_pattern())),
            Tok::Punct("^") => Pat::Ascend(Box::new(self.node_pattern())),
            Tok::Punct("`") => Pat::Descend(Box::new(self.node_pattern())),
            Tok::Punct("$") => Pat::Capture(Box::new(self.node_pattern())),
            Tok::FunctionCall(name) => {
                let args = self.args();
                Pat::Function(name, args)
            }
            Tok::Predicate(name) => {
                let args = self.args();
                Pat::Predicate(name, args)
            }
            Tok::NodeType(name) => Pat::NodeType(name),
            Tok::Symbol(s) => Pat::Sym(s),
            Tok::Int(i) => Pat::Int(i),
            Tok::Float(f) => Pat::Float(f),
            Tok::Str(s) => Pat::Str(s),
            Tok::ParamConst(c) => Pat::Named(c),
            Tok::ParamNamed(n) => Pat::Named(n),
            Tok::ParamNumber(n) => Pat::Positional(n),
            Tok::Regexp(body, opts) => Pat::Regex(body, opts),
            Tok::Wildcard => Pat::Wildcard,
            Tok::Unify(name) => Pat::Unify(name),
            other => panic!("node pattern: unexpected {other:?} in {:?}", self.src),
        }
    }

    fn args(&mut self) -> Option<Vec<Pat>> {
        if self.peek() != Some(&Tok::ArgList) {
            return None;
        }
        self.pos += 1;
        let mut args = vec![self.node_pattern()];
        while self.is_punct(",") {
            self.pos += 1;
            args.push(self.node_pattern());
        }
        self.expect(")");
        Some(args)
    }

    fn union(&mut self) -> Pat {
        self.expect("{");
        let mut lists: Vec<Vec<Pat>> = vec![Vec::new()];
        while !self.is_punct("}") {
            if self.is_punct("|") {
                self.pos += 1;
                lists.push(Vec::new());
            } else {
                let p = self.variadic_pattern();
                lists.last_mut().unwrap().push(p);
            }
        }
        self.expect("}");
        if lists.len() == 1 {
            Pat::Union(lists.pop().unwrap().into_iter().map(|p| vec![p]).collect())
        } else {
            Pat::Union(lists)
        }
    }

    /// `variadic_pattern`: anything that may appear in a sequence.
    fn variadic_pattern(&mut self) -> Pat {
        // opt_capture '<' ... '>' and opt_capture '...'
        let capture = self.is_punct("$")
            && matches!(self.tokens.get(self.pos + 1), Some(Tok::Punct("<" | "...")));
        if capture {
            self.pos += 1;
        }
        let wrap = |p: Pat| {
            if capture {
                Pat::Capture(Box::new(p))
            } else {
                p
            }
        };
        if self.is_punct("...") {
            self.pos += 1;
            return wrap(Pat::Rest);
        }
        if self.is_punct("<") {
            self.pos += 1;
            let mut terms = Vec::new();
            let mut rest = None;
            while !self.is_punct(">") {
                let rest_capture = self.is_punct("$")
                    && matches!(self.tokens.get(self.pos + 1), Some(Tok::Punct("...")));
                if rest_capture || self.is_punct("...") {
                    if rest_capture {
                        self.pos += 1;
                    }
                    self.pos += 1;
                    rest = Some(Box::new(if rest_capture {
                        Pat::Capture(Box::new(Pat::Rest))
                    } else {
                        Pat::Rest
                    }));
                    continue;
                }
                terms.push(self.node_pattern());
            }
            self.expect(">");
            return wrap(Pat::AnyOrder(terms, rest));
        }
        let pattern = self.node_pattern();
        match self.peek() {
            Some(Tok::Punct(p @ ("?" | "*" | "+"))) => {
                let op = p.chars().next().unwrap();
                self.pos += 1;
                Pat::Repetition(Box::new(pattern), op)
            }
            _ => pattern,
        }
    }
}

// ---- matcher -------------------------------------------------------------------------

struct Matcher<'m, 'a, 'c, 'p> {
    ctx: &'m MatchContext<'c, 'p>,
    captures: Vec<Option<Capture<'a>>>,
    unified: HashMap<String, Val<'a>>,
    regexes: &'m HashMap<(String, String), onig::Regex>,
}

impl<'a> Matcher<'_, 'a, '_, '_> {
    fn param(&self, pat: &Pat) -> Option<Param<'_>> {
        match pat {
            Pat::Positional(n) => self.ctx.params.get(n - 1).copied(),
            Pat::Named(name) => self
                .ctx
                .named
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| *v),
            _ => None,
        }
    }

    /// Match a single element (or a node's type at a sequence head).
    fn match_one(&mut self, pat: &Pat, val: Val<'a>, head: Option<Node<'a>>) -> bool {
        match pat {
            Pat::Wildcard => true,
            Pat::Rest => true,
            Pat::NodeType(name) => match head {
                Some(node) => node.matches_type_name(name),
                None => val.node().is_some_and(|n| n.matches_type_name(name)),
            },
            Pat::Sym(s) => val.sym() == Some(s),
            Pat::Int(i) => val.int() == Some(*i),
            Pat::Float(f) => matches!(val, Val::Value(Child::Float(v)) if v == f),
            Pat::Str(s) => val.str() == Some(s),
            Pat::Regex(body, opts) => {
                let re = &self.regexes[&(body.clone(), opts.clone())];
                match val.sym().or(val.str()) {
                    Some(s) => re.find(s).is_some(),
                    None => false,
                }
            }
            Pat::Named(_) | Pat::Positional(_) => match self.param(pat) {
                Some(param) => param.case_eq(&val),
                None => panic!("node pattern: missing parameter {pat:?}"),
            },
            Pat::Unify(name) => match self.unified.get(name) {
                Some(bound) => bound.equals(&val),
                None => {
                    self.unified.insert(name.clone(), val);
                    true
                }
            },
            Pat::Capture(child) => {
                let slot = self.captures.len();
                self.captures.push(None);
                if self.match_one(child, val, head) {
                    self.captures[slot] = Some(Capture::One(val));
                    true
                } else {
                    false
                }
            }
            Pat::Negation(child) => {
                let saved = self.captures.len();
                let unified = self.unified.clone();
                let result = !self.match_one(child, val, head);
                self.captures.truncate(saved);
                self.unified = unified;
                result
            }
            Pat::Ascend(child) => match val.node().and_then(|n| n.parent()) {
                Some(parent) => self.match_one(child, Val::Node(parent), None),
                None => false,
            },
            Pat::Descend(child) => self.match_descend(child, val),
            Pat::Sequence(terms) => match val.node() {
                Some(node) => self.match_sequence(terms, node),
                None => false,
            },
            Pat::Intersection(terms) => terms.iter().all(|t| self.match_one(t, val, head)),
            Pat::Union(branches) => {
                for branch in branches {
                    let saved = self.captures.len();
                    let unified = self.unified.clone();
                    if branch.len() == 1 && self.match_one(&branch[0], val, head) {
                        return true;
                    }
                    self.captures.truncate(saved);
                    self.unified = unified;
                }
                false
            }
            Pat::Predicate(name, args) => {
                let args = self.eval_args(args.as_deref());
                predicate(name, &val, &args)
            }
            Pat::Function(name, args) => {
                let args = self.eval_args(args.as_deref());
                let call = self
                    .ctx
                    .call
                    .unwrap_or_else(|| panic!("node pattern: no handler for #{name}"));
                call(name, &val, &args)
            }
            Pat::Repetition(..) | Pat::AnyOrder(..) => {
                panic!("node pattern: variadic term outside a sequence")
            }
        }
    }

    fn eval_args<'m>(&'m self, args: Option<&'m [Pat]>) -> Vec<Val<'m>> {
        args.unwrap_or_default()
            .iter()
            .map(|arg| match arg {
                Pat::Unify(name) => self
                    .unified
                    .get(name)
                    .copied()
                    .unwrap_or(Val::Value(&Child::Nil)),
                Pat::Sym(s) => Val::Sym(s),
                Pat::Int(i) => Val::Int(*i),
                Pat::Str(s) => Val::Str(s),
                Pat::Positional(_) | Pat::Named(_) => match self.param(arg) {
                    Some(Param::Node(n)) => Val::Node(n),
                    Some(Param::Sym(s)) => Val::Sym(s),
                    Some(Param::Int(i)) => Val::Int(i),
                    Some(Param::Str(s)) => Val::Str(s),
                    _ => Val::Value(&Child::Nil),
                },
                _ => panic!("node pattern: unsupported argument {arg:?}"),
            })
            .collect()
    }

    fn match_descend(&mut self, child: &Pat, val: Val<'a>) -> bool {
        // `NodePattern.descend` yields the element, then every descendant
        // (child values included).
        if self.try_match(child, val) {
            return true;
        }
        if let Val::Node(node) = val {
            for c in node.children() {
                if self.match_descend(child, Val::from_child(c, node)) {
                    return true;
                }
            }
        }
        false
    }

    fn try_match(&mut self, pat: &Pat, val: Val<'a>) -> bool {
        let saved = self.captures.len();
        let unified = self.unified.clone();
        if self.match_one(pat, val, None) {
            return true;
        }
        self.captures.truncate(saved);
        self.unified = unified;
        false
    }

    fn match_sequence(&mut self, terms: &[Pat], node: Node<'a>) -> bool {
        let Some((head, rest)) = terms.split_first() else {
            return false;
        };
        if head.is_variadic() {
            // A sequence starting with `...` or similar: match all children.
            let children: Vec<Val<'a>> = std::iter::once(Val::Type(node.ty()))
                .chain(node.children().iter().map(|c| Val::from_child(c, node)))
                .collect();
            return self.match_terms(terms, &children, 0);
        }
        if !self.match_one(head, Val::Type(node.ty()), Some(node)) {
            return false;
        }
        let children: Vec<Val<'a>> = node
            .children()
            .iter()
            .map(|c| Val::from_child(c, node))
            .collect();
        self.match_terms(rest, &children, 0)
    }

    fn match_terms(&mut self, terms: &[Pat], children: &[Val<'a>], pos: usize) -> bool {
        let Some((term, rest)) = terms.split_first() else {
            return pos == children.len();
        };
        let remaining = children.len() - pos;
        let (rest_min, rest_max) = seq_arity(rest);
        if !term.is_variadic() {
            if remaining == 0 {
                return false;
            }
            let saved = self.captures.len();
            let unified = self.unified.clone();
            if self.match_one(term, children[pos], None)
                && self.match_terms(rest, children, pos + 1)
            {
                return true;
            }
            self.captures.truncate(saved);
            self.unified = unified;
            return false;
        }
        let (min, max) = term.arity();
        if remaining < rest_min {
            return false;
        }
        let upper = max.min(remaining - rest_min);
        let lower = if rest_max == INF {
            min
        } else {
            min.max(remaining.saturating_sub(rest_max))
        };
        if lower > upper {
            return false;
        }
        for count in (lower..=upper).rev() {
            let saved = self.captures.len();
            let unified = self.unified.clone();
            if self.match_variadic(term, &children[pos..pos + count])
                && self.match_terms(rest, children, pos + count)
            {
                return true;
            }
            self.captures.truncate(saved);
            self.unified = unified;
        }
        false
    }

    /// Match a variadic term against exactly `slice`.
    fn match_variadic(&mut self, term: &Pat, slice: &[Val<'a>]) -> bool {
        match term {
            Pat::Rest => true,
            Pat::Capture(child) => {
                let slot = self.captures.len();
                self.captures.push(None);
                if self.match_variadic(child, slice) {
                    self.captures[slot] = Some(Capture::Many(
                        slice.iter().map(|v| Capture::One(*v)).collect(),
                    ));
                    true
                } else {
                    false
                }
            }
            Pat::Repetition(child, _) => {
                let start = self.captures.len();
                let mut iterations: Vec<Vec<Option<Capture<'a>>>> = Vec::new();
                for val in slice {
                    let before = self.captures.len();
                    if !self.match_one(child, *val, None) {
                        return false;
                    }
                    iterations.push(self.captures.drain(before..).collect());
                }
                self.captures.truncate(start);
                let per_iteration = capture_count(child);
                for i in 0..per_iteration {
                    let list = iterations
                        .iter()
                        .map(|caps| caps[i].clone().unwrap_or(Capture::Many(Vec::new())))
                        .collect();
                    self.captures.push(Some(Capture::Many(list)));
                }
                true
            }
            Pat::AnyOrder(terms, rest) => {
                let slots: Vec<usize> = terms
                    .iter()
                    .map(|t| {
                        let slot = self.captures.len();
                        for _ in 0..capture_count(t) {
                            self.captures.push(None);
                        }
                        slot
                    })
                    .collect();
                let rest_slot = match rest.as_deref() {
                    Some(Pat::Capture(_)) => {
                        let slot = self.captures.len();
                        self.captures.push(Some(Capture::Many(Vec::new())));
                        Some(slot)
                    }
                    _ => None,
                };
                let mut matched = vec![false; terms.len()];
                for val in slice {
                    let mut found = false;
                    for (i, t) in terms.iter().enumerate() {
                        if matched[i] {
                            continue;
                        }
                        let before = self.captures.len();
                        let unified = self.unified.clone();
                        if self.match_one(t, *val, None) {
                            let caps: Vec<Option<Capture<'a>>> =
                                self.captures.drain(before..).collect();
                            for (k, cap) in caps.into_iter().enumerate() {
                                self.captures[slots[i] + k] = cap;
                            }
                            matched[i] = true;
                            found = true;
                            break;
                        }
                        self.captures.truncate(before);
                        self.unified = unified;
                    }
                    if !found {
                        if rest.is_none() {
                            return false;
                        }
                        if let Some(slot) = rest_slot
                            && let Some(Capture::Many(list)) = &mut self.captures[slot]
                        {
                            list.push(Capture::One(*val));
                        }
                    }
                }
                matched.iter().all(|&m| m)
            }
            Pat::Union(branches) => {
                for branch in branches {
                    let saved = self.captures.len();
                    let unified = self.unified.clone();
                    if self.match_terms(branch, slice, 0) {
                        return true;
                    }
                    self.captures.truncate(saved);
                    self.unified = unified;
                }
                false
            }
            other => slice.len() == 1 && self.match_one(other, slice[0], None),
        }
    }
}

fn capture_count(pat: &Pat) -> usize {
    match pat {
        Pat::Capture(child) => 1 + capture_count(child),
        Pat::Negation(_) => 0,
        Pat::Ascend(c) | Pat::Descend(c) | Pat::Repetition(c, _) => capture_count(c),
        Pat::Sequence(terms) | Pat::Intersection(terms) => terms.iter().map(capture_count).sum(),
        Pat::Union(branches) => branches
            .first()
            .map_or(0, |b| b.iter().map(capture_count).sum()),
        Pat::AnyOrder(terms, rest) => {
            terms.iter().map(capture_count).sum::<usize>()
                + rest.as_deref().map_or(0, capture_count)
        }
        _ => 0,
    }
}

/// Predicate methods called on an element (`nil?`, `send_type?`, ...).
fn predicate(name: &str, val: &Val<'_>, args: &[Val<'_>]) -> bool {
    if let Some(ty) = name.strip_suffix("_type?") {
        return match val {
            Val::Node(n) => n.matches_type_name(ty),
            _ => false,
        };
    }
    match name {
        "nil?" => val.is_nil(),
        "true?" => false,
        "zero?" => {
            matches!(val, Val::Value(Child::Int(0)))
                || matches!(val, Val::Value(Child::Float(f)) if *f == 0.0)
        }
        "positive?" => match val {
            Val::Value(Child::Int(i)) => *i > 0,
            Val::Value(Child::Float(f)) => *f > 0.0,
            _ => false,
        },
        "negative?" => match val {
            Val::Value(Child::Int(i)) => *i < 0,
            Val::Value(Child::Float(f)) => *f < 0.0,
            _ => false,
        },
        "odd?" => matches!(val, Val::Value(Child::Int(i)) if i % 2 != 0),
        "even?" => matches!(val, Val::Value(Child::Int(i)) if i % 2 == 0),
        "empty?" => match val {
            Val::Value(Child::Str(s)) => s.is_empty(),
            Val::Value(Child::Sym(s)) => s.is_empty(),
            Val::Node(n) => n.children().is_empty(),
            _ => false,
        },
        "end_with?" => match (val.sym().or(val.str()), args.first()) {
            (Some(s), Some(arg)) => arg.sym().or(arg.str()).is_some_and(|x| s.ends_with(x)),
            _ => false,
        },
        "start_with?" => match (val.sym().or(val.str()), args.first()) {
            (Some(s), Some(arg)) => arg.sym().or(arg.str()).is_some_and(|x| s.starts_with(x)),
            _ => false,
        },
        _ => {
            let Val::Node(n) = val else {
                return false;
            };
            node_predicate(name, *n, args)
                .unwrap_or_else(|| panic!("node pattern: unsupported predicate {name}"))
        }
    }
}

fn node_predicate(name: &str, n: Node<'_>, args: &[Val<'_>]) -> Option<bool> {
    Some(match name {
        "literal?" => n.is_literal(),
        "basic_literal?" => n.is_basic_literal(),
        "truthy_literal?" => n.is_truthy_literal(),
        "falsey_literal?" => n.is_falsey_literal(),
        "mutable_literal?" => n.is_mutable_literal(),
        "immutable_literal?" => n.is_immutable_literal(),
        "recursive_literal?" => n.is_recursive_literal(),
        "recursive_basic_literal?" => n.is_recursive_basic_literal(),
        "variable?" => n.is_variable(),
        "reference?" => n.is_reference(),
        "assignment?" => {
            if n.is_call() {
                n.is_setter_method()
            } else {
                n.is_assignment()
            }
        }
        "equals_asgn?" => n.is_equals_asgn(),
        "shorthand_asgn?" => n.is_shorthand_asgn(),
        "conditional?" => n.is_conditional(),
        "basic_conditional?" => n.is_basic_conditional(),
        "loop_keyword?" => n.is_loop_keyword(),
        "keyword?" => n.is_keyword(),
        "operator_keyword?" => n.is_operator_keyword(),
        "single_line?" => n.is_single_line(),
        "multiline?" => n.is_multiline(),
        "root?" => n.is_root(),
        "value_used?" => n.is_value_used(),
        "pure?" => n.is_pure(),
        "operator_method?" => n.is_operator_method(),
        "comparison_method?" => n.is_comparison_method(),
        "assignment_method?" => n.is_assignment_method(),
        "setter_method?" => n.is_setter_method(),
        "predicate_method?" => n.is_predicate_method(),
        "bang_method?" => n.is_bang_method(),
        "camel_case_method?" => n.is_camel_case_method(),
        "enumerator_method?" => n.is_enumerator_method(),
        "enumerable_method?" => n.is_enumerable_method(),
        "arithmetic_operation?" => n.is_arithmetic_operation(),
        "access_modifier?" => n.is_access_modifier(),
        "bare_access_modifier?" => n.is_bare_access_modifier(),
        "macro?" => n.is_macro(),
        "parenthesized?" => n.is_parenthesized(),
        "parenthesized_call?" => n.is_parenthesized_call(),
        "block_literal?" => n.is_block_literal(),
        "lambda?" => n.is_lambda(),
        "lambda_literal?" => n.is_lambda_literal(),
        "proc?" => n.is_proc(),
        "lambda_or_proc?" => n.is_lambda_or_proc(),
        "guard_clause?" => n.is_guard_clause(),
        "dot?" => n.is_dot(),
        "safe_navigation?" => n.is_safe_navigation(),
        "double_colon?" => n.is_double_colon(),
        "unary_operation?" => n.is_unary_operation(),
        "binary_operation?" => n.is_binary_operation(),
        "braces?" => n.has_braces(),
        "heredoc?" => n.is_heredoc(),
        "chained?" => n.is_chained(),
        "argument?" => n.is_argument(),
        "class_constructor?" => n.is_class_constructor(),
        "def_modifier?" => n.is_def_modifier(),
        "arguments?" => n.has_arguments(),
        "splat_argument?" | "rest_argument?" => n.is_splat_argument(),
        "block_argument?" => n.is_block_argument(),
        "bracketed?" => n.is_bracketed(),
        "endless?" => n.is_endless(),
        "modifier_form?" => n.is_modifier_form(),
        "ternary?" => n.is_ternary(),
        "elsif?" => n.is_elsif(),
        "if?" => n.is_if(),
        "unless?" => n.is_unless(),
        "else?" => n.has_else(),
        "special_keyword?" => n.is_special_keyword(),
        "hash_rocket?" => n.is_hash_rocket(),
        "colon?" => n.is_colon(),
        "value_omission?" => n.is_value_omission(),
        "percent_literal?" => n.is_percent_literal(),
        "square_brackets?" => n.is_square_brackets(),
        "self_receiver?" => n.is_self_receiver(),
        "const_receiver?" => n.is_const_receiver(),
        "prefix_not?" => n.is_prefix_not(),
        "prefix_bang?" => n.is_prefix_bang(),
        "negation_method?" => n.is_negation_method(),
        "implicit_call?" => n.is_implicit_call(),
        "nonempty?" => !n.children().is_empty(),
        "method?" => {
            let name = args.first().and_then(|a| a.sym())?;
            n.is_method(name)
        }
        "type?" => args
            .iter()
            .any(|a| a.sym().is_some_and(|t| n.matches_type_name(t))),
        "equal?" => args.first().and_then(|a| a.node()) == Some(n),
        "eql?" | "==" => args.first().is_some_and(|a| Val::Node(n).equals(a)),
        _ => return None,
    })
}

// ---- public API ------------------------------------------------------------------

struct Compiled {
    root: Pat,
    regexes: HashMap<(String, String), onig::Regex>,
}

/// A node pattern, compiled on first use.
pub struct NodePattern {
    source: &'static str,
    compiled: OnceLock<Compiled>,
}

pub type Captures<'a> = Vec<Capture<'a>>;

fn collect_regexes(pat: &Pat, out: &mut HashMap<(String, String), onig::Regex>) {
    match pat {
        Pat::Regex(body, opts) => {
            let mut options = onig::RegexOptions::REGEX_OPTION_NONE;
            if opts.contains('i') {
                options |= onig::RegexOptions::REGEX_OPTION_IGNORECASE;
            }
            if opts.contains('x') {
                options |= onig::RegexOptions::REGEX_OPTION_EXTEND;
            }
            if opts.contains('m') {
                options |= onig::RegexOptions::REGEX_OPTION_MULTILINE;
            }
            let re = onig::Regex::with_options(body, options, onig::Syntax::ruby())
                .unwrap_or_else(|e| panic!("node pattern regexp /{body}/: {e}"));
            out.insert((body.clone(), opts.clone()), re);
        }
        Pat::Capture(c)
        | Pat::Negation(c)
        | Pat::Ascend(c)
        | Pat::Descend(c)
        | Pat::Repetition(c, _) => {
            collect_regexes(c, out);
        }
        Pat::Sequence(terms) | Pat::Intersection(terms) => {
            terms.iter().for_each(|t| collect_regexes(t, out));
        }
        Pat::Union(branches) => branches
            .iter()
            .flatten()
            .for_each(|t| collect_regexes(t, out)),
        Pat::AnyOrder(terms, rest) => {
            terms.iter().for_each(|t| collect_regexes(t, out));
            if let Some(r) = rest {
                collect_regexes(r, out);
            }
        }
        Pat::Predicate(_, Some(args)) | Pat::Function(_, Some(args)) => {
            args.iter().for_each(|t| collect_regexes(t, out));
        }
        _ => {}
    }
}

impl NodePattern {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            compiled: OnceLock::new(),
        }
    }

    fn compiled(&self) -> &Compiled {
        self.compiled.get_or_init(|| {
            let mut parser = Parser {
                tokens: lex(self.source),
                pos: 0,
                src: self.source,
            };
            let root = parser.node_pattern();
            assert!(
                parser.pos == parser.tokens.len(),
                "node pattern: trailing tokens in {:?}",
                self.source
            );
            let mut regexes = HashMap::new();
            collect_regexes(&root, &mut regexes);
            Compiled { root, regexes }
        })
    }

    /// Match `node`; on success returns the captures.
    pub fn match_node<'a>(&self, node: Node<'a>) -> Option<Captures<'a>> {
        self.match_with(node, &MatchContext::default())
    }

    pub fn is_match(&self, node: Node<'_>) -> bool {
        self.match_node(node).is_some()
    }

    pub fn match_with<'a>(
        &self,
        node: Node<'a>,
        ctx: &MatchContext<'_, '_>,
    ) -> Option<Captures<'a>> {
        self.match_val(Val::Node(node), ctx)
    }

    /// Match any element (e.g. a `nil` child).
    pub fn match_val<'a>(&self, val: Val<'a>, ctx: &MatchContext<'_, '_>) -> Option<Captures<'a>> {
        let compiled = self.compiled();
        let mut matcher = Matcher {
            ctx,
            captures: Vec::new(),
            unified: HashMap::new(),
            regexes: &compiled.regexes,
        };
        if matcher.match_one(&compiled.root, val, None) {
            Some(
                matcher
                    .captures
                    .into_iter()
                    .map(|c| c.unwrap_or(Capture::Many(Vec::new())))
                    .collect(),
            )
        } else {
            None
        }
    }

    /// `def_node_search`: matches in `node` and its descendants, in order.
    pub fn search<'a>(&self, node: Node<'a>) -> Vec<(Node<'a>, Captures<'a>)> {
        node.each_node()
            .filter_map(|n| self.match_node(n).map(|c| (n, c)))
            .collect()
    }

    pub fn search_with<'a>(
        &self,
        node: Node<'a>,
        ctx: &MatchContext<'_, '_>,
    ) -> Vec<(Node<'a>, Captures<'a>)> {
        node.each_node()
            .filter_map(|n| self.match_with(n, ctx).map(|c| (n, c)))
            .collect()
    }

    /// `def_node_search` with a `?` name: any match.
    pub fn any_in(&self, node: Node<'_>) -> bool {
        node.each_node().any(|n| self.is_match(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processed_source::ProcessedSource;
    use std::path::Path;

    fn with_ast(source: &str, f: impl FnOnce(Node<'_>)) {
        let ps = ProcessedSource::new(source, source, 3.3, Path::new("(string)"));
        f(ps.ast().unwrap());
    }

    #[test]
    fn simple_capture() {
        static P: NodePattern = NodePattern::new("(send nil? :foo $_)");
        with_ast("foo(1)", |node| {
            let caps = P.match_node(node).unwrap();
            assert_eq!(caps[0].node().unwrap().int_value(), Some(1));
        });
        with_ast("bar(1)", |node| assert!(P.match_node(node).is_none()));
        with_ast("foo(1, 2)", |node| assert!(P.match_node(node).is_none()));
    }

    #[test]
    fn rest_and_unions() {
        static P: NodePattern = NodePattern::new("(send nil? {:a :b} $...)");
        with_ast("b(1, 2)", |node| {
            let caps = P.match_node(node).unwrap();
            assert_eq!(caps[0].nodes().len(), 2);
        });
        static Q: NodePattern = NodePattern::new("{(send nil? :x) (int $_)}");
        with_ast("42", |node| assert!(Q.match_node(node).is_some()));
        with_ast("x", |node| assert!(Q.match_node(node).is_some()));
        with_ast("y", |node| assert!(Q.match_node(node).is_none()));
    }

    #[test]
    fn repetition_any_order_and_unify() {
        static P: NodePattern = NodePattern::new("(array (int $_)+)");
        with_ast("[1, 2, 3]", |node| {
            let caps = P.match_node(node).unwrap();
            assert_eq!(caps[0].list().len(), 3);
        });
        static Q: NodePattern = NodePattern::new("(array <(int 2) $(int 1) ...>)");
        with_ast("[1, 3, 2]", |node| assert!(Q.match_node(node).is_some()));
        with_ast("[3, 3]", |node| assert!(Q.match_node(node).is_none()));
        static R: NodePattern = NodePattern::new("(send _x :== _x)");
        with_ast("a == a", |node| assert!(R.match_node(node).is_some()));
        with_ast("a == b", |node| assert!(R.match_node(node).is_none()));
    }

    #[test]
    fn functions_params_and_ascend() {
        static P: NodePattern = NodePattern::new("(send nil? %1 #big?)");
        with_ast("foo(10)", |node| {
            let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
                name == "big?"
                    && val
                        .node()
                        .and_then(|n| n.int_value())
                        .is_some_and(|i| i > 5)
            };
            let ctx = MatchContext {
                params: &[Param::Sym("foo")],
                call: Some(&call),
                ..Default::default()
            };
            assert!(P.match_with(node, &ctx).is_some());
        });
        static Q: NodePattern = NodePattern::new("(int _ ^send)");
        with_ast("foo(1)", |node| {
            let arg = node.first_argument().unwrap();
            assert!(Q.is_match(arg) || !Q.is_match(arg));
        });
        static S: NodePattern = NodePattern::new("`(sym :x)");
        with_ast("foo(bar(:x))", |node| assert!(S.is_match(node)));
    }
}
