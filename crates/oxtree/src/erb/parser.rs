//! Port of w_syntax_tree-erb 0.12.0's `lib/syntax_tree/erb/parser.rb` and
//! `nodes.rb`.
//!
//! The parser is a recursive descent over a lazily produced token stream in
//! which `maybe` swallows "missing token" failures *without rewinding*, so a
//! failed alternative keeps whatever tokens it consumed. That behavior is
//! reproduced exactly, as is the tokenizer raising only when the parser
//! peeks at the offending position.

#![allow(clippy::large_enum_variant)]

use crate::node::Ast;
use crate::parser::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T {
    BlankLine,
    NewLine,
    HtmlComment,
    Doctype,
    ErbComment,
    ErbOpen,
    SlashOpen,
    Open,
    Whitespace,
    Text,
    ErbIf,
    ErbUnless,
    ErbElsif,
    ErbElse,
    ErbCase,
    ErbWhen,
    ErbEnd,
    ErbCode,
    ErbDoClose,
    ErbClose,
    ErbYield,
    StringCloseSingleQuote,
    StringCloseDoubleQuote,
    StringOpenSingleQuote,
    StringOpenDoubleQuote,
    Close,
    SpecialClose,
    SlashClose,
    Slash,
    Equals,
    Name,
    Eof,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Loc {
    pub start_char: usize,
    pub end_char: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
}

impl Loc {
    pub fn to(&self, other: &Loc) -> Loc {
        Loc {
            start_char: self.start_char,
            end_char: other.end_char,
            start_line: self.start_line,
            end_line: self.end_line.max(other.end_line),
            start_column: self.start_column,
            end_column: other.end_column,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub ty: T,
    pub value: String,
    pub location: Loc,
}

#[derive(Debug, Clone)]
pub struct NewLine {
    pub count: usize,
    pub location: Loc,
}

/// `ErbClose` / `ErbDoClose`.
#[derive(Debug, Clone)]
pub struct ErbClose {
    pub closing: Token,
    pub new_line: Option<NewLine>,
    pub do_close: bool,
    pub location: Loc,
}

/// `ErbContent`: the Ruby code of a tag, parsed by syntax_tree.
#[derive(Debug, Clone)]
pub struct ErbContent {
    pub value: Option<std::rc::Rc<Ast>>,
}

#[derive(Debug, Clone)]
pub struct ErbNode {
    pub opening_tag: Token,
    pub keyword: Option<Token>,
    pub content: ErbContent,
    pub closing_tag: ErbClose,
    /// `Element#new_line` (rarely set; `ErbNode#new_line` is the closing
    /// tag's).
    pub new_line: Option<NewLine>,
    pub location: Loc,
}

#[derive(Debug, Clone)]
pub struct OpeningTag {
    pub opening: Token,
    pub name: Token,
    pub attributes: Vec<Attr>,
    pub closing: Token,
    pub new_line: Option<NewLine>,
    pub location: Loc,
}

#[derive(Debug, Clone)]
pub enum Attr {
    Erb(Box<Elem>),
    Html(HtmlAttribute),
}

#[derive(Debug, Clone)]
pub struct HtmlAttribute {
    pub key: Token,
    pub equals: Option<Token>,
    pub value: Option<HtmlString>,
    pub location: Loc,
}

#[derive(Debug, Clone)]
pub struct HtmlString {
    pub opening: Option<Token>,
    /// Tokens and ERB tags.
    pub contents: Vec<StringPart>,
    pub closing: Option<Token>,
    pub location: Loc,
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Token(Token),
    Erb(Box<Elem>),
}

#[derive(Debug, Clone)]
pub struct ClosingTag {
    pub opening: Token,
    pub name: Token,
    pub closing: Token,
    pub new_line: Option<NewLine>,
    pub location: Loc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    ErbBlock,
    ErbIf,
    ErbUnless,
    ErbElsif,
    ErbElse,
    ErbCase,
    ErbCaseWhen,
}

#[derive(Debug, Clone)]
pub enum Elem {
    Html {
        opening: OpeningTag,
        elements: Vec<Elem>,
        closing: Option<ClosingTag>,
        location: Loc,
    },
    Erb(ErbNode),
    ErbEnd(ErbNode),
    Block {
        kind: BlockKind,
        opening: ErbNode,
        elements: Vec<Elem>,
        closing: Box<Elem>,
        location: Loc,
    },
    CharData {
        value: Token,
        new_line: Option<NewLine>,
        location: Loc,
    },
    NewLine(NewLine),
    HtmlComment {
        token: Token,
        new_line: Option<NewLine>,
        location: Loc,
    },
    ErbComment {
        token: Token,
        new_line: Option<NewLine>,
        location: Loc,
    },
    Doctype {
        opening: Token,
        name: Token,
        closing: Token,
        new_line: Option<NewLine>,
        location: Loc,
    },
    /// Only inside ERB tag contents.
    ErbYield {
        new_line: Option<NewLine>,
        location: Loc,
    },
}

pub struct Document {
    pub elements: Vec<Elem>,
}

pub const YIELD_PLACEHOLDER: &str = "qqqqy";

impl Elem {
    pub fn location(&self) -> Loc {
        match self {
            Elem::Html { location, .. }
            | Elem::Block { location, .. }
            | Elem::CharData { location, .. }
            | Elem::HtmlComment { location, .. }
            | Elem::ErbComment { location, .. }
            | Elem::Doctype { location, .. }
            | Elem::ErbYield { location, .. } => *location,
            Elem::Erb(node) | Elem::ErbEnd(node) => node.location,
            Elem::NewLine(n) => n.location,
        }
    }

    /// `#new_line`
    pub fn new_line(&self) -> Option<&NewLine> {
        match self {
            Elem::Html {
                opening, closing, ..
            } => match closing {
                None => opening.new_line.as_ref(),
                Some(c) => c.new_line.as_ref(),
            },
            Elem::Block { closing, .. } => closing.new_line(),
            Elem::Erb(node) | Elem::ErbEnd(node) => node.closing_tag.new_line.as_ref(),
            Elem::CharData { new_line, .. }
            | Elem::HtmlComment { new_line, .. }
            | Elem::ErbComment { new_line, .. }
            | Elem::Doctype { new_line, .. }
            | Elem::ErbYield { new_line, .. } => new_line.as_ref(),
            Elem::NewLine(_) => None,
        }
    }

    /// `#without_new_line`
    pub fn without_new_line(&self) -> Elem {
        let mut copy = self.clone();
        match &mut copy {
            Elem::Html {
                opening, closing, ..
            } => match closing {
                None => opening.new_line = None,
                Some(c) => c.new_line = None,
            },
            Elem::Block { closing, .. } => **closing = closing.without_new_line(),
            Elem::Erb(node) | Elem::ErbEnd(node) => node.closing_tag.new_line = None,
            Elem::CharData {
                value, new_line, ..
            } => {
                *new_line = None;
                value.value = value.value.trim_end().to_string();
            }
            Elem::HtmlComment { new_line, .. }
            | Elem::ErbComment { new_line, .. }
            | Elem::Doctype { new_line, .. }
            | Elem::ErbYield { new_line, .. } => *new_line = None,
            Elem::NewLine(_) => {}
        }
        copy
    }

    /// `#skip?`
    fn skip(&self) -> bool {
        match self {
            Elem::CharData { value, .. } => value.value.trim().is_empty(),
            _ => false,
        }
    }
}

// ----------------------------------------------------------------------
// Tokenizer (`make_tokens`)
// ----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Outside,
    ErbStart,
    Erb,
    StringSingleQuote,
    StringDoubleQuote,
    Inside,
}

struct RawToken {
    ty: T,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
}

/// `\s` in Ruby.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}' | '\u{b}')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The length of a run of chars matching `f` at the start of `s`.
fn run(s: &str, f: impl Fn(char) -> bool) -> usize {
    s.char_indices()
        .find(|&(_, c)| !f(c))
        .map_or(s.len(), |(i, _)| i)
}

/// `/\A\s*keyword/`
fn keyword_prefix(s: &str, keyword: &str) -> Option<usize> {
    let ws = run(s, is_space);
    if s[ws..].starts_with(keyword) {
        Some(ws + keyword.len())
    } else {
        None
    }
}

/// `/\Ado\b(\s*\|[\w\s,]+\|)?\s*-?%>/`
fn erb_do_close(s: &str) -> Option<usize> {
    let rest = s.strip_prefix("do")?;
    if rest.chars().next().is_some_and(is_word) {
        return None;
    }
    let mut i = 2;
    // optional (\s*\|[\w\s,]+\|)
    let ws = run(&s[i..], is_space);
    if s[i + ws..].starts_with('|') {
        let inner_start = i + ws + 1;
        let inner = run(&s[inner_start..], |c| is_word(c) || is_space(c) || c == ',');
        if inner > 0 && s[inner_start + inner..].starts_with('|') {
            i = inner_start + inner + 1;
        }
    }
    i += run(&s[i..], is_space);
    if s[i..].starts_with("-%>") {
        Some(i + 3)
    } else if s[i..].starts_with("%>") {
        Some(i + 2)
    } else {
        None
    }
}

fn tokenize(source: &str) -> (Vec<RawToken>, Option<ParseError>) {
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut column_index = 0usize;
    let mut line = 1usize;
    let mut state = vec![State::Outside];

    macro_rules! emit {
        ($ty:expr, $len:expr) => {{
            tokens.push(RawToken {
                ty: $ty,
                start: index,
                end: index + $len,
                line,
                column: column_index,
            });
        }};
    }
    let error = |message: String, line: usize, column: usize| ParseError {
        message,
        lineno: line,
        column,
    };

    while index < source.len() {
        let rest = &source[index..];
        let first = rest.chars().next().unwrap();
        let len: usize;
        match *state.last().unwrap() {
            State::Outside => {
                if rest.starts_with("\n\n") {
                    len = run(rest, |c| c == '\n');
                    emit!(T::BlankLine, len);
                    line += len;
                } else if first == '\n' {
                    len = 1;
                    emit!(T::NewLine, len);
                    line += 1;
                } else if rest.starts_with("<!--")
                    && let Some(end) = rest.find("-->")
                {
                    len = end + 3;
                    emit!(T::HtmlComment, len);
                    line += rest[..len].matches('\n').count();
                } else if rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
                    len = 9;
                    emit!(T::Doctype, len);
                    state.push(State::Inside);
                } else if rest.starts_with("<%#")
                    && let Some(end) = rest[3..].find("%>")
                {
                    len = 3 + end + 2;
                    emit!(T::ErbComment, len);
                } else if rest.starts_with("<%") {
                    len = if rest.starts_with("<%==") {
                        4
                    } else if rest.starts_with("<%=") || rest.starts_with("<%-") {
                        3
                    } else {
                        2
                    };
                    emit!(T::ErbOpen, len);
                    state.push(State::ErbStart);
                } else if rest.starts_with("</") {
                    len = 2;
                    emit!(T::SlashOpen, len);
                    state.push(State::Inside);
                } else if first == '<' {
                    len = 1;
                    emit!(T::Open, len);
                    state.push(State::Inside);
                } else if matches!(first, ' ' | '\t' | '\r') {
                    len = run(rest, |c| matches!(c, ' ' | '\t' | '\r'));
                    emit!(T::Whitespace, len);
                } else {
                    // /\A(?!\s+$)[^<\n]+/
                    let text = run(rest, |c| c != '<' && c != '\n');
                    let blank_to_eol = {
                        let ws = run(rest, is_space);
                        ws > 0 && {
                            // \s+$ with backtracking: some prefix of the
                            // whitespace run ends right before a newline or
                            // at the end of the string.
                            rest[..ws].contains('\n') || ws == rest.len()
                        }
                    };
                    if text == 0 || blank_to_eol {
                        return (
                            tokens,
                            Some(error(
                                format!("Unexpected character: {first}"),
                                line,
                                column_index,
                            )),
                        );
                    }
                    len = text;
                    emit!(T::Text, len);
                }
            }
            State::ErbStart => {
                let keywords = [
                    ("if", T::ErbIf),
                    ("unless", T::ErbUnless),
                    ("elsif", T::ErbElsif),
                    ("else", T::ErbElse),
                    ("case", T::ErbCase),
                    ("when", T::ErbWhen),
                    ("end", T::ErbEnd),
                ];
                state.pop();
                state.push(State::Erb);
                match keywords
                    .iter()
                    .find_map(|(kw, ty)| keyword_prefix(rest, kw).map(|l| (l, *ty)))
                {
                    Some((l, ty)) => {
                        len = l;
                        emit!(ty, len);
                    }
                    None => continue,
                }
            }
            State::Erb => {
                if first == '\n' {
                    len = run(rest, |c| c == '\n');
                    emit!(T::ErbCode, len);
                    line += len;
                } else if let Some(l) = erb_do_close(rest) {
                    len = l;
                    emit!(T::ErbDoClose, len);
                    state.pop();
                } else if rest.starts_with("-%>") || rest.starts_with("%>") {
                    len = if first == '-' { 3 } else { 2 };
                    emit!(T::ErbClose, len);
                    state.pop();
                } else if rest.starts_with("yield")
                    && !rest[5..].chars().next().is_some_and(is_word)
                {
                    len = 5;
                    emit!(T::ErbYield, len);
                } else if is_word(first) {
                    len = run(rest, is_word);
                    emit!(T::ErbCode, len);
                } else {
                    len = first.len_utf8();
                    emit!(T::ErbCode, len);
                    index += len;
                    column_index += 1;
                    continue;
                }
            }
            State::StringSingleQuote | State::StringDoubleQuote => {
                let quote = if *state.last().unwrap() == State::StringSingleQuote {
                    '\''
                } else {
                    '"'
                };
                if matches!(first, ' ' | '\t' | '\n' | '\r') {
                    len = whitespace_run_crlf(rest);
                    if len == 0 {
                        return (
                            tokens,
                            Some(error(quote_error(first, quote), line, column_index)),
                        );
                    }
                    emit!(T::Whitespace, len);
                    line += rest[..len].matches('\n').count();
                } else if first == quote {
                    len = 1;
                    emit!(
                        if quote == '\'' {
                            T::StringCloseSingleQuote
                        } else {
                            T::StringCloseDoubleQuote
                        },
                        len
                    );
                    state.pop();
                } else if rest.starts_with("<%") {
                    len = if rest.starts_with("<%=") { 3 } else { 2 };
                    emit!(T::ErbOpen, len);
                    state.push(State::ErbStart);
                } else if first != '<' {
                    len = run(rest, |c| c != '<' && c != quote);
                    emit!(T::Text, len);
                } else {
                    return (
                        tokens,
                        Some(error(quote_error(first, quote), line, column_index)),
                    );
                }
            }
            State::Inside => {
                if matches!(first, ' ' | '\t' | '\r' | '\n') {
                    len = run(rest, |c| matches!(c, ' ' | '\t' | '\r' | '\n'));
                    line += rest[..len].matches('\n').count();
                } else if rest.starts_with("-%>") || rest.starts_with("%>") {
                    len = if first == '-' { 3 } else { 2 };
                    emit!(T::ErbClose, len);
                    state.pop();
                } else if first == '>' {
                    len = 1;
                    emit!(T::Close, len);
                    state.pop();
                } else if rest.starts_with("?>") {
                    len = 2;
                    emit!(T::SpecialClose, len);
                    state.pop();
                } else if rest.starts_with("/>") {
                    len = 2;
                    emit!(T::SlashClose, len);
                    state.pop();
                } else if first == '/' {
                    len = 1;
                    emit!(T::Slash, len);
                } else if first == '=' {
                    len = 1;
                    emit!(T::Equals, len);
                } else if let Some(l) = name_token(rest) {
                    len = l;
                    emit!(T::Name, len);
                } else if rest.starts_with("<%") {
                    len = 2;
                    emit!(T::ErbOpen, len);
                    state.push(State::ErbStart);
                } else if first == '"' {
                    len = 1;
                    emit!(T::StringOpenDoubleQuote, len);
                    state.push(State::StringDoubleQuote);
                } else if first == '\'' {
                    len = 1;
                    emit!(T::StringOpenSingleQuote, len);
                    state.push(State::StringSingleQuote);
                } else {
                    return (
                        tokens,
                        Some(error(
                            format!("Unexpected character, {first}, when parsing HTML- or ERB-tag"),
                            line,
                            column_index,
                        )),
                    );
                }
            }
        }
        let matched = &source[index..index + len];
        index += len;
        column_index = match matched.rfind('\n') {
            Some(i) => matched[..i].chars().count(),
            None => column_index + matched.chars().count(),
        };
    }
    tokens.push(RawToken {
        ty: T::Eof,
        start: index,
        end: index,
        line,
        column: column_index,
    });
    (tokens, None)
}

fn quote_error(first: char, quote: char) -> String {
    if quote == '\'' {
        format!("Unexpected character, {first}, when looking for closing single quote")
    } else {
        format!("Unexpected character, {first}, when looking for closing double quote")
    }
}

/// `/\A(?: |\t|\n|\r\n)+/m`
fn whitespace_run_crlf(s: &str) -> usize {
    let mut i = 0;
    let bytes = s.as_bytes();
    loop {
        match bytes.get(i) {
            Some(b' ' | b'\t' | b'\n') => i += 1,
            Some(b'\r') if bytes.get(i + 1) == Some(&b'\n') => i += 2,
            _ => return i,
        }
    }
}

/// `/\A[@#]*[:\w\.\-\_]+\b/`
fn name_token(s: &str) -> Option<usize> {
    let prefix = run(s, |c| c == '@' || c == '#');
    let body_chars = |c: char| c == ':' || is_word(c) || c == '.' || c == '-' || c == '_';
    let body = run(&s[prefix..], body_chars);
    if body == 0 {
        return None;
    }
    // Backtrack until the end sits on a word boundary.
    let candidate = &s[prefix..prefix + body];
    let next_is_word = |end: usize| s[prefix + end..].chars().next().is_some_and(is_word);
    let mut ends: Vec<usize> = candidate
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    ends.reverse();
    for end in ends {
        let last = candidate[..end].chars().last().unwrap();
        let boundary = is_word(last) != next_is_word(end);
        if boundary {
            return Some(prefix + end);
        }
    }
    None
}

// ----------------------------------------------------------------------
// Parser
// ----------------------------------------------------------------------

enum Fail {
    /// `MissingTokenError`, rescued by `maybe`/`many`.
    Missing,
    Parse(ParseError),
}

type R<T> = Result<T, Fail>;

fn parse_error(message: impl Into<String>, line: usize, column: usize) -> Fail {
    Fail::Parse(ParseError {
        message: message.into(),
        lineno: line,
        column,
    })
}

pub struct Parser<'s> {
    source: &'s str,
    tokens: Vec<RawToken>,
    tokenizer_error: Option<ParseError>,
    cursor: usize,
    found_doctype: bool,
}

impl<'s> Parser<'s> {
    pub fn new(source: &'s str) -> Self {
        let (tokens, tokenizer_error) = tokenize(source);
        Parser {
            source,
            tokens,
            tokenizer_error,
            cursor: 0,
            found_doctype: false,
        }
    }

    pub fn parse(mut self) -> Result<Document, ParseError> {
        let elements = match self.many(|p| p.parse_any_tag()) {
            Ok(elements) => elements,
            Err(Fail::Parse(e)) => return Err(e),
            Err(Fail::Missing) => unreachable!(),
        };
        Ok(Document { elements })
    }

    fn peek(&self) -> R<&RawToken> {
        match self.tokens.get(self.cursor) {
            Some(token) => Ok(token),
            None => match &self.tokenizer_error {
                Some(e) => Err(Fail::Parse(e.clone())),
                // Past EOF: Enumerator#peek raises StopIteration.
                None => Err(Fail::Missing),
            },
        }
    }

    fn consume(&mut self, expected: T) -> R<Token> {
        let token = self.peek()?;
        if token.ty != expected {
            return Err(Fail::Missing);
        }
        let value = &self.source[token.start..token.end];
        let rindex = value.rfind('\n');
        let length = value.chars().count();
        let location = Loc {
            start_char: token.start,
            end_char: token.end,
            start_line: token.line,
            end_line: token.line + value.matches('\n').count(),
            start_column: token.column,
            end_column: match rindex {
                Some(r) => length - value[..r].chars().count(),
                None => token.column + length,
            },
        };
        let token = Token {
            ty: token.ty,
            value: value.to_string(),
            location,
        };
        self.cursor += 1;
        Ok(token)
    }

    fn maybe<X>(&mut self, f: impl FnOnce(&mut Self) -> R<X>) -> R<Option<X>> {
        match f(self) {
            Ok(x) => Ok(Some(x)),
            Err(Fail::Missing) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn many<X>(&mut self, mut f: impl FnMut(&mut Self) -> R<X>) -> R<Vec<X>> {
        let mut items = Vec::new();
        loop {
            match f(self) {
                Ok(x) => items.push(x),
                Err(Fail::Missing) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(items)
    }

    fn atleast<X>(result: Option<X>) -> R<X> {
        result.ok_or(Fail::Missing)
    }

    fn parse_any_tag(&mut self) -> R<Elem> {
        loop {
            let tag = self.parse_any_alternative()?;
            if let Elem::Doctype { location, .. } = &tag {
                if self.found_doctype {
                    return Err(parse_error(
                        "Duplicate doctype declaration",
                        location.start_line,
                        location.start_column,
                    ));
                }
                self.found_doctype = true;
            }
            if matches!(tag, Elem::NewLine(_)) {
                continue;
            }
            if !tag.skip() {
                return Ok(tag);
            }
        }
    }

    fn parse_any_alternative(&mut self) -> R<Elem> {
        if let Some(t) = self.maybe(|p| p.parse_doctype())? {
            return Ok(t);
        }
        if let Some(t) = self.maybe(|p| p.parse_html_comment())? {
            return Ok(t);
        }
        if let Some(t) = self.maybe(|p| p.parse_erb_tag())? {
            return Ok(t);
        }
        if let Some(t) = self.maybe(|p| p.parse_erb_comment())? {
            return Ok(t);
        }
        if let Some(t) = self.maybe(|p| p.parse_html_element())? {
            return Ok(t);
        }
        if let Some(t) = self.maybe(|p| p.parse_new_line().map(Elem::NewLine))? {
            return Ok(t);
        }
        let chardata = self.maybe(|p| p.parse_chardata())?.flatten();
        Self::atleast(chardata)
    }

    fn parse_until_erb(&mut self, is_end: impl Fn(&Elem) -> bool) -> R<Vec<Elem>> {
        let mut items = Vec::new();
        loop {
            let result = self.parse_any_tag()?;
            let done = is_end(&result);
            items.push(result);
            if done {
                break;
            }
        }
        Ok(items)
    }

    fn parse_html_opening_tag(&mut self) -> R<OpeningTag> {
        let opening = self.consume(T::Open)?;
        let name = self.consume(T::Name)?;
        if name.value.starts_with(['@', ':', '#']) {
            return Err(parse_error(
                format!("Invalid HTML-tag name {}", name.value),
                name.location.start_line,
                name.location.start_column,
            ));
        }
        let attributes = self.many(|p| {
            if let Some(erb) = p.maybe(|p| p.parse_erb_tag())? {
                return Ok(Attr::Erb(Box::new(erb)));
            }
            let attr = p.maybe(|p| p.parse_html_attribute())?;
            Self::atleast(attr).map(Attr::Html)
        })?;
        let closing = match self.maybe(|p| p.consume(T::Close))? {
            Some(c) => c,
            None => Self::atleast(self.maybe(|p| p.consume(T::SlashClose))?)?,
        };
        let new_line = self.maybe(|p| p.parse_new_line())?;
        self.maybe(|p| p.consume(T::Whitespace))?;
        let location = opening.location.to(&closing.location);
        Ok(OpeningTag {
            opening,
            name,
            attributes,
            closing,
            new_line,
            location,
        })
    }

    fn parse_html_closing(&mut self) -> R<ClosingTag> {
        let opening = self.consume(T::SlashOpen)?;
        let name = self.consume(T::Name)?;
        let closing = self.consume(T::Close)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let location = opening.location.to(&closing.location);
        Ok(ClosingTag {
            opening,
            name,
            closing,
            new_line,
            location,
        })
    }

    fn parse_html_element(&mut self) -> R<Elem> {
        let opening = self.parse_html_opening_tag()?;
        if opening.closing.value == "/>" || is_void_element(&opening.name.value) {
            let location = opening.location;
            return Ok(Elem::Html {
                opening,
                elements: vec![],
                closing: None,
                location,
            });
        }
        let elements = self.many(|p| p.parse_any_tag())?;
        let closing = self.maybe(|p| p.parse_html_closing())?;
        let Some(closing) = closing else {
            return Err(parse_error(
                format!("Missing closing tag for <{}>", opening.name.value),
                opening.location.start_line,
                opening.location.start_column,
            ));
        };
        if closing.name.value != opening.name.value {
            return Err(parse_error(
                format!(
                    "Expected closing tag for <{}> but got <{}>",
                    opening.name.value, closing.name.value
                ),
                closing.location.start_line,
                closing.location.start_column,
            ));
        }
        let location = opening.location.to(&closing.location);
        Ok(Elem::Html {
            opening,
            elements,
            closing: Some(closing),
            location,
        })
    }

    fn parse_erb_case(&mut self, erb_node: ErbNode) -> R<Elem> {
        let mut elements = self
            .maybe(|p| {
                p.parse_until_erb(|e| match e {
                    Elem::Block { kind, .. } => {
                        matches!(kind, BlockKind::ErbCaseWhen | BlockKind::ErbElse)
                    }
                    Elem::ErbEnd(_) => true,
                    _ => false,
                })
            })?
            .unwrap_or_default();
        let erb_tag = elements.pop();
        let valid = match &erb_tag {
            Some(Elem::Block { kind, .. }) => {
                matches!(kind, BlockKind::ErbCaseWhen | BlockKind::ErbElse)
            }
            Some(Elem::ErbEnd(_)) => true,
            _ => false,
        };
        if !valid {
            let location = erb_tag.as_ref().map_or(erb_node.location, |t| t.location());
            return Err(parse_error(
                format!(
                    "No matching ERB-tag for the <% {} %>",
                    erb_node.keyword.as_ref().map_or("", |k| &k.value)
                ),
                location.start_line,
                location.start_column,
            ));
        }
        let erb_tag = erb_tag.unwrap();
        let kind = match erb_node.keyword.as_ref().map(|k| k.ty) {
            Some(T::ErbCase) => BlockKind::ErbCase,
            Some(T::ErbWhen) => BlockKind::ErbCaseWhen,
            _ => {
                return Err(parse_error(
                    "No matching when- or else-tag for the case-tag",
                    erb_node.location.start_line,
                    erb_node.location.start_column,
                ));
            }
        };
        let location = erb_node.location.to(&erb_tag.location());
        Ok(Elem::Block {
            kind,
            opening: erb_node,
            elements,
            closing: Box::new(erb_tag),
            location,
        })
    }

    fn parse_erb_if(&mut self, erb_node: ErbNode) -> R<Elem> {
        self.maybe(|p| p.consume(T::Whitespace))?;
        let mut elements = self
            .maybe(|p| {
                p.parse_until_erb(|e| match e {
                    Elem::Block { kind, .. } => {
                        matches!(kind, BlockKind::ErbElsif | BlockKind::ErbElse)
                    }
                    Elem::ErbEnd(_) => true,
                    _ => false,
                })
            })?
            .unwrap_or_default();
        let erb_tag = elements.pop();
        // ErbControl: ErbIf, ErbUnless, ErbElsif, ErbElse, ErbCase, ErbCaseWhen
        let valid = match &erb_tag {
            Some(Elem::Block { kind, .. }) => *kind != BlockKind::ErbBlock,
            Some(Elem::ErbEnd(_)) => true,
            _ => false,
        };
        if !valid {
            return Err(parse_error(
                "No matching ERB-tag for the <% if %>",
                erb_node.location.start_line,
                erb_node.location.start_column,
            ));
        }
        let erb_tag = erb_tag.unwrap();
        let kind = match erb_node.keyword.as_ref().map(|k| k.ty) {
            Some(T::ErbIf) => BlockKind::ErbIf,
            Some(T::ErbUnless) => BlockKind::ErbUnless,
            Some(T::ErbElsif) => BlockKind::ErbElsif,
            _ => {
                return Err(parse_error(
                    "No matching <% elsif %> or <% else %> for the <% if %>",
                    erb_node.location.start_line,
                    erb_node.location.start_column,
                ));
            }
        };
        let location = erb_node.location.to(&erb_tag.location());
        Ok(Elem::Block {
            kind,
            opening: erb_node,
            elements,
            closing: Box::new(erb_tag),
            location,
        })
    }

    fn parse_erb_else(&mut self, erb_node: ErbNode) -> R<Elem> {
        let mut elements = self
            .maybe(|p| p.parse_until_erb(|e| matches!(e, Elem::ErbEnd(_))))?
            .unwrap_or_default();
        let erb_end = elements.pop();
        let Some(erb_end @ Elem::ErbEnd(_)) = erb_end else {
            return Err(parse_error(
                "No matching <% end %> for the <% else %>",
                erb_node.location.start_line,
                erb_node.location.start_column,
            ));
        };
        let location = erb_node.location.to(&erb_end.location());
        Ok(Elem::Block {
            kind: BlockKind::ErbElse,
            opening: erb_node,
            elements,
            closing: Box::new(erb_end),
            location,
        })
    }

    fn parse_erb_end(&mut self, erb_node: ErbNode) -> R<Elem> {
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let content = ErbContent::parse_items(&[])?;
        Ok(Elem::ErbEnd(ErbNode {
            opening_tag: erb_node.opening_tag,
            keyword: erb_node.keyword,
            content,
            closing_tag: erb_node.closing_tag,
            new_line,
            location: erb_node.location,
        }))
    }

    fn parse_erb_tag(&mut self) -> R<Elem> {
        let opening_tag = self.consume(T::ErbOpen)?;
        let mut keyword = None;
        for ty in [
            T::ErbIf,
            T::ErbUnless,
            T::ErbElsif,
            T::ErbElse,
            T::ErbEnd,
            T::ErbCase,
            T::ErbWhen,
        ] {
            if let Some(k) = self.maybe(|p| p.consume(ty))? {
                keyword = Some(k);
                break;
            }
        }
        let mut content = self.parse_until_erb_close()?;
        let closing_tag = match content.pop() {
            Some(ContentItem::Close(close)) => close,
            other => {
                let location = other.map(|o| o.location()).unwrap_or_default();
                return Err(parse_error(
                    format!(
                        "No matching closing tag for the <% {} %>",
                        keyword.as_ref().map_or("", |k| k.value.as_str())
                    ),
                    location.start_line,
                    location.start_column,
                ));
            }
        };
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let location = opening_tag.location.to(&closing_tag.location);
        let erb_node = ErbNode::new(
            opening_tag,
            keyword,
            &content,
            closing_tag,
            new_line,
            location,
        )?;

        match erb_node.keyword.as_ref().map(|k| k.ty) {
            Some(T::ErbIf | T::ErbUnless | T::ErbElsif) => self.parse_erb_if(erb_node),
            Some(T::ErbCase | T::ErbWhen) => self.parse_erb_case(erb_node),
            Some(T::ErbElse) => self.parse_erb_else(erb_node),
            Some(T::ErbEnd) => self.parse_erb_end(erb_node),
            _ => {
                if erb_node.closing_tag.do_close {
                    let mut elements = self
                        .maybe(|p| p.parse_until_erb(|e| matches!(e, Elem::ErbEnd(_))))?
                        .unwrap_or_default();
                    let erb_end = elements.pop();
                    let Some(erb_end @ Elem::ErbEnd(_)) = erb_end else {
                        return Err(parse_error(
                            "No matching <% end %> for the <% do %>",
                            erb_node.location.start_line,
                            erb_node.location.start_column,
                        ));
                    };
                    let location = erb_node.location.to(&erb_end.location());
                    Ok(Elem::Block {
                        kind: BlockKind::ErbBlock,
                        opening: erb_node,
                        elements,
                        closing: Box::new(erb_end),
                        location,
                    })
                } else {
                    Ok(Elem::Erb(erb_node))
                }
            }
        }
    }

    fn parse_until_erb_close(&mut self) -> R<Vec<ContentItem>> {
        let mut items = Vec::new();
        loop {
            let result = if let Some(c) = self.maybe(|p| p.parse_erb_close(T::ErbDoClose))? {
                ContentItem::Close(c)
            } else if let Some(c) = self.maybe(|p| p.parse_erb_close(T::ErbClose))? {
                ContentItem::Close(c)
            } else if let Some(y) = self.maybe(|p| p.parse_erb_yield())? {
                y
            } else {
                ContentItem::Code(Self::atleast(self.maybe(|p| p.consume(T::ErbCode))?)?)
            };
            let done = matches!(result, ContentItem::Close(_));
            items.push(result);
            if done {
                break;
            }
        }
        Ok(items)
    }

    fn parse_new_line(&mut self) -> R<NewLine> {
        let line_break = match self.maybe(|p| p.consume(T::BlankLine))? {
            Some(t) => t,
            None => Self::atleast(self.maybe(|p| p.consume(T::NewLine))?)?,
        };
        self.maybe(|p| p.consume(T::Whitespace))?;
        Ok(NewLine {
            location: line_break.location,
            count: line_break.value.matches('\n').count(),
        })
    }

    fn parse_erb_close(&mut self, ty: T) -> R<ErbClose> {
        let closing = self.consume(ty)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        Ok(ErbClose {
            location: closing.location,
            closing,
            new_line,
            do_close: ty == T::ErbDoClose,
        })
    }

    fn parse_erb_yield(&mut self) -> R<ContentItem> {
        let token = self.consume(T::ErbYield)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        Ok(ContentItem::Yield {
            location: token.location,
            new_line,
        })
    }

    fn parse_html_string(&mut self) -> R<HtmlString> {
        let opening = match self.maybe(|p| p.consume(T::StringOpenDoubleQuote))? {
            Some(o) => Some(o),
            None => self.maybe(|p| p.consume(T::StringOpenSingleQuote))?,
        };
        let Some(opening) = opening else {
            let value = self.consume(T::Name)?;
            let location = value.location;
            return Ok(HtmlString {
                opening: None,
                contents: vec![StringPart::Token(value)],
                closing: None,
                location,
            });
        };
        let contents = self.many(|p| {
            if let Some(t) = p.maybe(|p| p.consume(T::Text))? {
                return Ok(StringPart::Token(t));
            }
            if let Some(t) = p.maybe(|p| p.consume(T::Whitespace))? {
                return Ok(StringPart::Token(t));
            }
            let erb = p.maybe(|p| p.parse_erb_tag())?;
            Self::atleast(erb).map(|e| StringPart::Erb(Box::new(e)))
        })?;
        let closing = if opening.ty == T::StringOpenDoubleQuote {
            self.consume(T::StringCloseDoubleQuote)?
        } else {
            self.consume(T::StringCloseSingleQuote)?
        };
        let location = opening.location.to(&closing.location);
        Ok(HtmlString {
            opening: Some(opening),
            contents,
            closing: Some(closing),
            location,
        })
    }

    fn parse_html_attribute(&mut self) -> R<HtmlAttribute> {
        let key = self.consume(T::Name)?;
        let equals = self.maybe(|p| p.consume(T::Equals))?;
        match equals {
            None => {
                let location = key.location;
                Ok(HtmlAttribute {
                    key,
                    equals: None,
                    value: None,
                    location,
                })
            }
            Some(equals) => {
                let value = self.parse_html_string()?;
                let location = key.location.to(&value.location);
                Ok(HtmlAttribute {
                    key,
                    equals: Some(equals),
                    value: Some(value),
                    location,
                })
            }
        }
    }

    fn parse_chardata(&mut self) -> R<Option<Elem>> {
        let values = self.many(|p| {
            for ty in [
                T::StringOpenDoubleQuote,
                T::StringOpenSingleQuote,
                T::StringCloseDoubleQuote,
                T::StringCloseSingleQuote,
                T::Text,
                T::Whitespace,
            ] {
                if let Some(t) = p.maybe(|p| p.consume(ty))? {
                    return Ok(t);
                }
            }
            Err(Fail::Missing)
        })?;
        let token = if values.len() > 1 {
            let value: String = values.iter().map(|v| v.value.as_str()).collect();
            let location = values[0].location.to(&values.last().unwrap().location);
            Some(Token {
                ty: T::Text,
                value,
                location,
            })
        } else {
            values.into_iter().next()
        };
        let new_line = self.maybe(|p| p.parse_new_line())?;
        Ok(token.map(|token| Elem::CharData {
            location: token.location,
            value: token,
            new_line,
        }))
    }

    fn parse_doctype(&mut self) -> R<Elem> {
        let opening = self.consume(T::Doctype)?;
        let name = self.consume(T::Name)?;
        let closing = self.consume(T::Close)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let location = opening.location.to(&closing.location);
        Ok(Elem::Doctype {
            opening,
            name,
            closing,
            new_line,
            location,
        })
    }

    fn parse_html_comment(&mut self) -> R<Elem> {
        let token = self.consume(T::HtmlComment)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let location = token.location;
        Ok(Elem::HtmlComment {
            token,
            new_line,
            location,
        })
    }

    fn parse_erb_comment(&mut self) -> R<Elem> {
        let token = self.consume(T::ErbComment)?;
        let new_line = self.maybe(|p| p.parse_new_line())?;
        let location = token.location;
        Ok(Elem::ErbComment {
            token,
            new_line,
            location,
        })
    }
}

pub fn is_void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// An item of an ERB tag's contents, before the code is parsed.
enum ContentItem {
    Code(Token),
    Yield {
        location: Loc,
        #[allow(dead_code)]
        new_line: Option<NewLine>,
    },
    Close(ErbClose),
}

impl ContentItem {
    fn location(&self) -> Loc {
        match self {
            ContentItem::Code(t) => t.location,
            ContentItem::Yield { location, .. } => *location,
            ContentItem::Close(c) => c.location,
        }
    }
}

impl ErbContent {
    fn code(items: &[ContentItem], keyword: Option<&Token>) -> String {
        let mut code = String::new();
        if let Some(keyword) = keyword {
            code.push_str(&keyword.value);
        }
        for item in items {
            match item {
                ContentItem::Code(t) => code.push_str(&t.value),
                ContentItem::Yield { .. } => code.push_str(YIELD_PLACEHOLDER),
                ContentItem::Close(c) => code.push_str(&c.closing.value),
            }
        }
        code
    }

    fn parse_code(code: &str) -> Result<ErbContent, ParseError> {
        let ast = crate::parser::parse(code.trim())?;
        Ok(ErbContent {
            value: Some(std::rc::Rc::new(ast)),
        })
    }

    fn parse_items(items: &[ContentItem]) -> R<ErbContent> {
        Self::parse_code(&Self::code(items, None)).map_err(Fail::Parse)
    }

    /// `ErbContent#blank?`
    pub fn blank(&self) -> bool {
        match &self.value {
            None => true,
            Some(ast) => statement_children(ast).is_empty(),
        }
    }
}

/// The program's statements without void statements.
pub fn statement_children(ast: &Ast) -> Vec<crate::node::NodeId> {
    use crate::node::Kind;
    let Kind::Program { statements } = ast.kind(ast.root) else {
        return vec![];
    };
    match ast.kind(*statements) {
        Kind::Statements { body } => body
            .iter()
            .copied()
            .filter(|&n| !matches!(ast.kind(n), Kind::VoidStmt))
            .collect(),
        _ => vec![],
    }
}

impl ErbNode {
    fn new(
        opening_tag: Token,
        keyword: Option<Token>,
        content: &[ContentItem],
        closing_tag: ErbClose,
        new_line: Option<NewLine>,
        location: Loc,
    ) -> R<ErbNode> {
        let mut keyword = keyword.map(|k| Token {
            ty: k.ty,
            value: k.value.trim().to_string(),
            location: k.location,
        });
        // prepare_content
        let parsed = match ErbContent::parse_code(&ErbContent::code(content, None)) {
            Ok(c) => c,
            Err(_) => match ErbContent::parse_code(&ErbContent::code(content, keyword.as_ref())) {
                Ok(c) => {
                    keyword = None;
                    c
                }
                Err(error) => {
                    let opening_location = opening_tag.location;
                    let content_location =
                        content.first().map_or(opening_location, |c| c.location());
                    let lineno = opening_tag.location.start_line + error.lineno - 1;
                    let column = if opening_location.start_line == error.lineno {
                        opening_location.start_column + error.column
                    } else {
                        content_location.start_column + error.column
                    }
                    .saturating_sub(1);
                    return Err(parse_error(
                        format!("Could not parse ERB-tag: {}", error.message),
                        lineno,
                        column,
                    ));
                }
            },
        };
        Ok(ErbNode {
            opening_tag,
            keyword,
            content: parsed,
            closing_tag,
            new_line,
            location,
        })
    }
}
