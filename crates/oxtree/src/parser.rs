//! Port of syntax_tree's `lib/syntax_tree/parser.rb`, driven by Prism instead
//! of Ripper.
//!
//! syntax_tree builds its tree from Ripper events and computes every node's
//! location from the tokens those events consume. Formatting depends on
//! those locations (blank lines between statements, comment placement), so
//! this module rebuilds the same tree shape from Prism's AST (following
//! Prism's own `Prism::Translation::Ripper` for the Prism -> Ripper mapping)
//! and computes locations with the same formulas as the Ripper event
//! handlers. Comments are attached exactly like upstream: first by
//! `Statements#bind` (in the same bottom-up order as Ripper's reductions),
//! then by `attach_comments` on the finished program.

use ruby_prism as pm;

use crate::node::{Ast, CallOperator, Kind, Location, NilOr, Node, NodeId};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub lineno: usize,
    pub column: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(source: &str) -> Result<Ast, ParseError> {
    let result = pm::parse(source.as_bytes());
    if let Some(error) = result.errors().next() {
        let start = error.location().start() as usize;
        let builder = Builder::new(source);
        let line = builder.line_of(start);
        return Err(ParseError {
            message: String::from_utf8_lossy(error.message()).into_owned(),
            lineno: line,
            column: start - builder.line_starts[line - 1],
        });
    }
    let mut builder = Builder::new(source);
    for comment in result.comments() {
        let loc = comment.location();
        builder.add_comment(
            loc.start() as usize,
            loc.end() as usize,
            matches!(comment.type_(), pm::CommentType::EmbDocComment),
        );
    }
    if let Some(data) = result.data_loc() {
        builder.end_content(data.start() as usize, data.end() as usize);
    }
    let program = result.node();
    let mut collector = TokenCollector::default();
    pm::Visit::visit(&mut collector, &program);
    collector.then.sort_unstable();
    collector.then.dedup();
    collector.pow.sort_unstable();
    collector.pow.dedup();
    builder.then_tokens = collector.then.into_iter().map(|p| (p, false)).collect();
    builder.pow_tokens = collector.pow.into_iter().map(|p| (p, false)).collect();
    let root = builder.program(&program);
    Ok(Ast {
        nodes: builder.nodes,
        source: source.to_string(),
        root,
    })
}

const BINARY_OPERATORS: &[&str] = &[
    "!=", "!~", "=~", "==", "===", "<=>", ">", ">=", "<", "<=", "&", "|", "^", ">>", "<<", "-",
    "+", "%", "/", "*", "**",
];

const KEYWORDS: &[&str] = &[
    "alias",
    "and",
    "begin",
    "BEGIN",
    "break",
    "case",
    "class",
    "def",
    "defined?",
    "do",
    "else",
    "elsif",
    "end",
    "END",
    "ensure",
    "false",
    "for",
    "if",
    "in",
    "module",
    "next",
    "nil",
    "not",
    "or",
    "redo",
    "rescue",
    "retry",
    "return",
    "self",
    "super",
    "then",
    "true",
    "undef",
    "unless",
    "until",
    "when",
    "while",
    "yield",
    "__ENCODING__",
    "__FILE__",
    "__LINE__",
];

/// A Prism location as a pair of byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

fn span(loc: &pm::Location<'_>) -> Span {
    Span {
        start: loc.start() as usize,
        end: loc.end() as usize,
    }
}

fn node_span(node: &pm::Node<'_>) -> Span {
    span(&node.location())
}

struct Builder<'s> {
    source: &'s str,
    bytes: &'s [u8],
    line_starts: Vec<usize>,
    nodes: Vec<Node>,
    /// The parser's running list of comments that haven't been attached yet.
    comments: Vec<NodeId>,
    end_content: Option<NodeId>,
    /// The line of the void_stmt of the next empty body to be bound, when it
    /// isn't the line of the closing token (class and module bodies, where
    /// the newline after the name is a terminator).
    next_void_line: Option<usize>,
    /// Every `then` keyword in the source, and whether a parser event has
    /// consumed it yet (Ripper's token list).
    then_tokens: Vec<(usize, bool)>,
    /// Every `**` operator token, and whether it has been consumed.
    pow_tokens: Vec<(usize, bool)>,
}

impl<'s> Builder<'s> {
    fn new(source: &'s str) -> Self {
        let bytes = source.as_bytes();
        let mut line_starts = vec![0];
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Builder {
            source,
            bytes,
            line_starts,
            nodes: Vec::new(),
            comments: Vec::new(),
            end_content: None,
            next_void_line: None,
            then_tokens: Vec::new(),
            pow_tokens: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Locations
    // ------------------------------------------------------------------

    fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }

    fn column_of(&self, offset: usize) -> usize {
        let line = self.line_of(offset);
        offset - self.line_starts[line - 1]
    }

    /// `Location.token`: a token that starts and ends on its first line.
    fn token_loc(&self, s: Span) -> Location {
        let line = self.line_of(s.start);
        let column = self.column_of(s.start);
        Location {
            start_line: line,
            start_char: s.start,
            start_column: column,
            end_line: line,
            end_char: s.end,
            end_column: column + (s.end - s.start),
        }
    }

    fn fixed_loc(&self, offset: usize) -> Location {
        Location::fixed(self.line_of(offset), offset, self.column_of(offset))
    }

    fn add(&mut self, kind: Kind, location: Location) -> NodeId {
        self.nodes.push(Node {
            kind,
            location,
            comments: Vec::new(),
        });
        (self.nodes.len() - 1) as NodeId
    }

    fn loc(&self, id: NodeId) -> Location {
        self.nodes[id as usize].location
    }

    fn to(&self, a: NodeId, b: NodeId) -> Location {
        self.loc(a).to(&self.loc(b))
    }

    fn kind(&self, id: NodeId) -> &Kind {
        &self.nodes[id as usize].kind
    }

    fn set_loc(&mut self, id: NodeId, loc: Location) {
        self.nodes[id as usize].location = loc;
    }

    fn slice(&self, s: Span) -> &'s str {
        &self.source[s.start..s.end]
    }

    fn text(&self, s: Span) -> String {
        self.slice(s).to_string()
    }

    // ------------------------------------------------------------------
    // Tokens
    // ------------------------------------------------------------------

    fn token(&mut self, s: Span, make: fn(String) -> Kind) -> NodeId {
        let loc = self.token_loc(s);
        let value = self.text(s);
        self.add(make(value), loc)
    }

    fn kw(&mut self, s: Span) -> NodeId {
        self.token(s, |value| Kind::Kw { value })
    }

    fn op(&mut self, s: Span) -> NodeId {
        self.token(s, |value| Kind::Op { value })
    }

    fn op_value(&mut self, s: Span, value: &str) -> NodeId {
        let loc = self.token_loc(s);
        self.add(
            Kind::Op {
                value: value.to_string(),
            },
            loc,
        )
    }

    fn ident(&mut self, s: Span) -> NodeId {
        self.token(s, |value| Kind::Ident { value })
    }

    fn tstring_content(&mut self, s: Span) -> NodeId {
        self.token(s, |value| Kind::TStringContent { value })
    }

    /// `visit_token` from Prism's Ripper translation: picks the scanner
    /// event for a method name, operator or symbol body.
    fn visit_token(&mut self, s: Span, allow_keywords: bool) -> NodeId {
        let value = self.text(s);
        let loc = self.token_loc(s);
        let kind = if value == "." {
            Kind::Period { value }
        } else if value == "`" {
            Kind::Backtick { value }
        } else if allow_keywords && KEYWORDS.contains(&value.as_str()) {
            Kind::Kw { value }
        } else if value.starts_with('_') {
            Kind::Ident { value }
        } else if value.chars().next().is_some_and(|c| c.is_uppercase())
            && value.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            Kind::Const { value }
        } else if value.starts_with("@@") {
            Kind::CVar { value }
        } else if value.starts_with('@') {
            Kind::IVar { value }
        } else if value.starts_with('$') {
            Kind::GVar { value }
        } else if value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_punctuation())
        {
            Kind::Op { value }
        } else {
            Kind::Ident { value }
        };
        self.add(kind, loc)
    }

    // ------------------------------------------------------------------
    // Comments
    // ------------------------------------------------------------------

    fn add_comment(&mut self, start: usize, end: usize, embdoc: bool) {
        let raw = &self.source[start..end];
        if embdoc {
            let value = raw.strip_suffix('\n').unwrap_or(raw).to_string();
            let mut end_char = start + value.len();
            if end_char > start {
                end_char = end_char.max(start);
            }
            let start_line = self.line_of(start);
            let end_line = self.line_of(end_char.saturating_sub(1).max(start));
            let loc = Location {
                start_line,
                start_char: start,
                start_column: self.column_of(start),
                end_line,
                end_char,
                end_column: self.column_of(end_char),
            };
            let id = self.add(
                Kind::EmbDoc {
                    value,
                    leading: false,
                    trailing: false,
                },
                loc,
            );
            self.comments.push(id);
            return;
        }
        let value = raw.strip_suffix('\r').unwrap_or(raw).to_string();
        // Ripper's comment token includes the newline and the location is
        // `size - 1` long.
        let has_newline = self.bytes.get(end) == Some(&b'\n');
        let size = if has_newline {
            value.len()
        } else {
            value.len().saturating_sub(1)
        };
        let loc = self.token_loc(Span {
            start,
            end: start + size,
        });
        let mut index = start as isize - 1;
        while index > -1 && matches!(self.bytes[index as usize], b'\t' | b' ') {
            index -= 1;
        }
        let inline = index != -1 && self.bytes[index as usize] != b'\n';
        let id = self.add(
            Kind::Comment {
                value,
                inline,
                leading: false,
                trailing: false,
            },
            loc,
        );
        self.comments.push(id);
    }

    fn comment_is_inline(&self, id: NodeId) -> bool {
        matches!(self.kind(id), Kind::Comment { inline: true, .. })
    }

    fn comment_is_ignore(&self, id: NodeId) -> bool {
        match self.kind(id) {
            Kind::Comment { value, .. } => is_stree_ignore(value),
            _ => false,
        }
    }

    fn end_content(&mut self, start: usize, _end: usize) {
        // `on___end__` receives the `__END__` line and keeps everything after
        // it as the value.
        let line_end = self.source[start..]
            .find('\n')
            .map_or(self.source.len(), |i| start + i + 1);
        let value = self.source[line_end..].to_string();
        let loc = self.token_loc(Span {
            start,
            end: line_end,
        });
        let id = self.add(Kind::EndContent { value }, loc);
        self.end_content = Some(id);
    }

    // ------------------------------------------------------------------
    // Statements helpers
    // ------------------------------------------------------------------

    /// `find_next_statement_start`
    fn find_next_statement_start(&self, position: usize) -> usize {
        let maximum = self.bytes.len();
        let mut index = position;
        while index <= maximum {
            match self.bytes.get(index) {
                Some(b'#') => {
                    return self.source[index + 1..]
                        .find('\n')
                        .map_or(maximum, |i| index + 1 + i);
                }
                Some(b' ') => {}
                _ => return position,
            }
            index += 1;
        }
        position
    }

    /// Approximates Ripper's lexer position when a parser event fires right
    /// after `position`: the end of the next (lookahead) token.
    fn after_next_token(&self, position: usize) -> usize {
        let mut i = position;
        while i < self.bytes.len() && matches!(self.bytes[i], b' ' | b'\t') {
            i += 1;
        }
        if i >= self.bytes.len() {
            return self.bytes.len();
        }
        let c = self.bytes[i];
        if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 {
            while i < self.bytes.len()
                && (self.bytes[i].is_ascii_alphanumeric()
                    || self.bytes[i] == b'_'
                    || self.bytes[i] >= 0x80)
            {
                i += 1;
            }
            i
        } else {
            i + 1
        }
    }

    /// The end of the next token after `position`, skipping whitespace,
    /// newlines and comments.
    fn next_token_end(&self, position: usize) -> usize {
        let mut i = position;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                b'#' => {
                    while i < self.bytes.len() && self.bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                _ => break,
            }
        }
        self.after_next_token(i)
    }

    /// `find_keyword_between(:then, ...)`, deleting the token.
    fn consume_then_between(&mut self, lo: usize, hi: usize) -> Option<Span> {
        let index = self
            .then_tokens
            .iter()
            .rposition(|&(pos, used)| !used && pos >= lo && pos < hi)?;
        self.then_tokens[index].1 = true;
        let pos = self.then_tokens[index].0;
        Some(Span {
            start: pos,
            end: pos + 4,
        })
    }

    /// `consume_operator(:**)` / `find_operator(:**)` + delete: the last
    /// unconsumed `**` lexed before `hi`.
    fn consume_pow_before(&mut self, hi: usize) -> Option<usize> {
        let index = self
            .pow_tokens
            .iter()
            .rposition(|&(pos, used)| !used && pos < hi)?;
        self.pow_tokens[index].1 = true;
        Some(self.pow_tokens[index].0)
    }

    /// `find_keyword(:then)`: the last `then` lexed so far.
    fn consume_then_before(&mut self, hi: usize) -> Option<Span> {
        self.consume_then_between(0, hi)
    }

    /// `Location.fixed` at Ripper's lexer position just after the token that
    /// follows `position` (newlines count as tokens), on that token's line.
    fn lookahead_loc(&self, position: usize) -> Location {
        let mut start = position;
        while start < self.bytes.len() && matches!(self.bytes[start], b' ' | b'\t') {
            start += 1;
        }
        let end = self.after_next_token(position);
        let line = self.line_of(start.min(self.bytes.len()));
        let column = end - self.line_starts[line - 1];
        Location::fixed(line, end, column)
    }

    fn void_stmt(&mut self, offset: usize) -> NodeId {
        let loc = self.fixed_loc(offset);
        self.add(Kind::VoidStmt, loc)
    }

    /// `on_stmts_new` + `on_stmts_add` for each statement.
    fn statements(&mut self, body: Vec<NodeId>, empty_offset: usize) -> NodeId {
        let body = if body.is_empty() {
            vec![self.void_stmt(empty_offset)]
        } else {
            body
        };
        let first = body[0];
        let last = *body.last().unwrap();
        let loc = self.to(first, last);
        self.add(Kind::Statements { body }, loc)
    }

    /// `Statements#bind`
    fn bind(&mut self, statements: NodeId, start_char: usize, end_char: usize) {
        let mut loc = self.loc(statements);
        loc.start_char = start_char;
        loc.start_column = self.column_of(start_char.min(self.bytes.len()));
        loc.end_char = end_char;
        loc.end_column = self.column_of(end_char.min(self.bytes.len()));
        self.set_loc(statements, loc);

        let first = match self.kind(statements) {
            Kind::Statements { body } => body.first().copied(),
            _ => None,
        };
        if let Some(void) = first
            && matches!(self.kind(void), Kind::VoidStmt)
        {
            // An empty body's void_stmt is reduced when Ripper reaches the
            // closing token, so its lines are that token's.
            let only_void =
                matches!(self.kind(statements), Kind::Statements { body } if body.len() == 1);
            if only_void {
                let line = self.next_void_line.take().unwrap_or_else(|| {
                    let at = end_char.min(self.bytes.len());
                    // At the end of the file Ripper is still on the last
                    // line, not the empty one after a trailing newline.
                    if at == self.bytes.len() && at > 0 && self.bytes[at - 1] == b'\n' {
                        self.line_of(at - 1)
                    } else {
                        self.line_of(at)
                    }
                });
                let mut vloc = self.loc(void);
                vloc.start_line = line;
                vloc.end_line = line;
                self.set_loc(void, vloc);
                loc.start_line = line;
                loc.end_line = line;
                self.set_loc(statements, loc);
            }
            let mut vloc = self.loc(void);
            vloc.start_char = start_char;
            vloc.start_column = loc.start_column;
            vloc.end_char = start_char;
            vloc.end_column = loc.end_column;
            self.set_loc(void, vloc);
        }

        self.attach_statement_comments(statements, start_char, end_char);
    }

    /// `Statements#bind_end`
    fn bind_end(&mut self, statements: NodeId, end_char: usize) {
        let mut loc = self.loc(statements);
        loc.end_char = end_char;
        loc.end_column = self.column_of(end_char.min(self.bytes.len()));
        self.set_loc(statements, loc);
    }

    /// `Statements#attach_comments`
    fn attach_statement_comments(
        &mut self,
        statements: NodeId,
        start_char: usize,
        end_char: usize,
    ) {
        let mut comment_index = 0;
        let mut body_index = 0;
        while comment_index < self.comments.len() {
            let comment = self.comments[comment_index];
            let cloc = self.loc(comment);
            if !self.comment_is_inline(comment)
                && start_char <= cloc.start_char
                && end_char >= cloc.end_char
                && !self.comment_is_ignore(comment)
            {
                let body = match self.kind(statements) {
                    Kind::Statements { body } => body.clone(),
                    _ => unreachable!(),
                };
                while let Some(&node) = body.get(body_index) {
                    if matches!(self.kind(node), Kind::VoidStmt)
                        || self.loc(node).start_char < cloc.start_char
                    {
                        body_index += 1;
                    } else {
                        break;
                    }
                }
                if body_index != 0
                    && self.loc(body[body_index - 1]).start_char < cloc.start_char
                    && self.loc(body[body_index - 1]).end_char > cloc.start_char
                {
                    comment_index += 1;
                } else {
                    self.comments.remove(comment_index);
                    if let Kind::Statements { body } = &mut self.nodes[statements as usize].kind {
                        body.insert(body_index, comment);
                    }
                }
            } else {
                comment_index += 1;
            }
        }
    }

    /// `BodyStmt#bind`
    fn bind_bodystmt(&mut self, bodystmt: NodeId, start_char: usize, end_char: usize) {
        let mut loc = self.loc(bodystmt);
        loc.start_char = start_char;
        loc.start_column = self.column_of(start_char.min(self.bytes.len()));
        loc.end_char = end_char;
        loc.end_column = self.column_of(end_char.min(self.bytes.len()));
        self.set_loc(bodystmt, loc);

        let (statements, rescue_clause, else_clause, ensure_clause) = match self.kind(bodystmt) {
            Kind::BodyStmt {
                statements,
                rescue_clause,
                else_clause,
                ensure_clause,
                ..
            } => (*statements, *rescue_clause, *else_clause, *ensure_clause),
            _ => unreachable!(),
        };
        let consequent = rescue_clause.or(else_clause).or(ensure_clause);
        let statements_end = consequent.map_or(end_char, |c| self.loc(c).start_char);
        self.bind(statements, start_char, statements_end);
        if consequent.is_none() {
            // on_bodystmt took its lines from the statements.
            let sloc = self.loc(statements);
            let mut loc = self.loc(bodystmt);
            loc.start_line = sloc.start_line;
            loc.end_line = sloc.end_line;
            self.set_loc(bodystmt, loc);
        }

        if let Some(rescue_clause) = rescue_clause {
            let consequent = else_clause.or(ensure_clause);
            let rescue_end = consequent.map_or(end_char, |c| self.loc(c).start_char);
            self.rescue_bind_end(rescue_clause, rescue_end);
        }
    }

    /// `Rescue#bind_end`
    fn rescue_bind_end(&mut self, rescue: NodeId, end_char: usize) {
        let mut loc = self.loc(rescue);
        loc.end_char = end_char;
        loc.end_column = self.column_of(end_char.min(self.bytes.len()));
        self.set_loc(rescue, loc);
        let (statements, consequent) = match self.kind(rescue) {
            Kind::Rescue {
                statements,
                consequent,
                ..
            } => (*statements, *consequent),
            _ => unreachable!(),
        };
        if let Some(next) = consequent {
            self.rescue_bind_end(next, end_char);
            let next_start = self.loc(next).start_char;
            self.bind_end(statements, next_start);
        } else {
            self.bind_end(statements, end_char);
        }
    }

    // ------------------------------------------------------------------
    // Program
    // ------------------------------------------------------------------

    fn program(&mut self, node: &pm::Node<'_>) -> NodeId {
        let program = node.as_program_node().expect("program node");
        let body: Vec<NodeId> = program
            .statements()
            .body()
            .iter()
            .map(|stmt| self.visit(&stmt))
            .collect();
        let statements = self.statements(body, 0);
        if let Some(end_content) = self.end_content
            && let Kind::Statements { body } = &mut self.nodes[statements as usize].kind
        {
            body.push(end_content);
        }
        let length = self.bytes.len();
        self.bind(statements, 0, length);
        let line_count = if self.source.is_empty() {
            1
        } else {
            self.source.split_inclusive('\n').count()
        };
        let last_line_start = if self.source.is_empty() {
            0
        } else {
            *self.line_starts.last().unwrap()
        };
        let loc = Location {
            start_line: 1,
            start_char: 0,
            start_column: 0,
            end_line: line_count,
            end_char: length,
            end_column: length - last_line_start,
        };
        let program = self.add(Kind::Program { statements }, loc);
        let remaining = std::mem::take(&mut self.comments);
        self.attach_comments(program, &remaining);
        program
    }

    /// `attach_comments`
    fn attach_comments(&mut self, program: NodeId, comments: &[NodeId]) {
        for &comment in comments {
            let (preceding, enclosing, following) = self.nearest_nodes(program, comment);
            let inline = self.comment_is_inline(comment);
            let target = if inline {
                if let Some(p) = preceding {
                    self.set_comment_flag(comment, false);
                    p
                } else if let Some(f) = following {
                    self.set_comment_flag(comment, true);
                    f
                } else {
                    enclosing.unwrap_or(program)
                }
            } else if let Some(f) = following {
                self.set_comment_flag(comment, true);
                f
            } else if let Some(p) = preceding {
                self.set_comment_flag(comment, false);
                p
            } else {
                enclosing.unwrap_or(program)
            };
            self.nodes[target as usize].comments.push(comment);
        }
    }

    fn set_comment_flag(&mut self, comment: NodeId, leading_flag: bool) {
        match &mut self.nodes[comment as usize].kind {
            Kind::Comment {
                leading, trailing, ..
            }
            | Kind::EmbDoc {
                leading, trailing, ..
            } => {
                if leading_flag {
                    *leading = true;
                } else {
                    *trailing = true;
                }
            }
            _ => {}
        }
    }

    /// `nearest_nodes`
    fn nearest_nodes(
        &self,
        node: NodeId,
        comment: NodeId,
    ) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
        let cloc = self.loc(comment);
        let comment_start = cloc.start_char;
        let comment_end = cloc.end_char;
        let child_nodes: Vec<NodeId> = crate::node::child_nodes(self.kind(node))
            .into_iter()
            .flatten()
            .collect();
        let mut preceding = None;
        let mut following = None;
        let mut left = 0;
        let mut right = child_nodes.len();
        while left < right {
            let middle = (left + right) / 2;
            let child = child_nodes[middle];
            let l = self.loc(child);
            if l.start_char <= comment_start && comment_end <= l.end_char {
                return self.nearest_nodes(child, comment);
            }
            if l.end_char <= comment_start {
                preceding = Some(child);
                left = middle + 1;
                continue;
            }
            if comment_end <= l.start_char {
                following = Some(child);
                right = middle;
                continue;
            }
            // "Comment location overlaps with node location": upstream raises.
            // Treat the node as enclosing instead.
            return self.nearest_nodes(child, comment);
        }
        (preceding, Some(node), following)
    }
}

pub fn is_stree_ignore(value: &str) -> bool {
    // /\A#\s*stree-ignore\s*\z/
    let Some(rest) = value.strip_prefix('#') else {
        return false;
    };
    rest.trim_matches(|c: char| c.is_ascii_whitespace()) == "stree-ignore"
}

// ----------------------------------------------------------------------
// Visiting Prism nodes
// ----------------------------------------------------------------------

/// The arguments of a call, Ripper-style: a block argument (`&blk`) is the
/// last argument rather than the block.
struct CallArguments {
    /// `Args`, `ArgsForward`, or nothing.
    arguments: Option<NodeId>,
    block: Option<NodeId>,
}

impl<'s> Builder<'s> {
    fn visit(&mut self, node: &pm::Node<'_>) -> NodeId {
        match node {
            pm::Node::AliasMethodNode { .. } => {
                let n = node.as_alias_method_node().unwrap();
                let left = self.visit(&n.new_name());
                let right = self.visit(&n.old_name());
                let keyword = self.kw(span(&n.keyword_loc()));
                let loc = self.to(keyword, right);
                self.add(Kind::AliasNode { left, right }, loc)
            }
            pm::Node::AliasGlobalVariableNode { .. } => {
                let n = node.as_alias_global_variable_node().unwrap();
                let left = self.alias_gvar(&n.new_name());
                let right = self.alias_gvar(&n.old_name());
                let keyword = self.kw(span(&n.keyword_loc()));
                let loc = self.to(keyword, right);
                self.add(Kind::AliasNode { left, right }, loc)
            }
            pm::Node::AlternationPatternNode { .. } => {
                let n = node.as_alternation_pattern_node().unwrap();
                let left = self.visit_pattern(&n.left());
                let right = self.visit_pattern(&n.right());
                self.binary(left, "|", right)
            }
            pm::Node::AndNode { .. } => {
                let n = node.as_and_node().unwrap();
                let left = self.visit(&n.left());
                let right = self.visit(&n.right());
                let operator = self.text(span(&n.operator_loc()));
                self.binary(left, &operator, right)
            }
            pm::Node::OrNode { .. } => {
                let n = node.as_or_node().unwrap();
                let left = self.visit(&n.left());
                let right = self.visit(&n.right());
                let operator = self.text(span(&n.operator_loc()));
                self.binary(left, &operator, right)
            }
            pm::Node::ArrayNode { .. } => self.array(node),
            pm::Node::ArrayPatternNode { .. } => self.array_pattern(node),
            pm::Node::AssocNode { .. } => {
                let n = node.as_assoc_node().unwrap();
                let key = self.visit(&n.key());
                let value = match n.value() {
                    pm::Node::ImplicitNode { .. } => None,
                    value => Some(self.visit(&value)),
                };
                let loc = match value {
                    Some(value) => self.to(key, value),
                    None => self.loc(key),
                };
                self.add(Kind::Assoc { key, value }, loc)
            }
            pm::Node::AssocSplatNode { .. } => {
                let n = node.as_assoc_splat_node().unwrap();
                let value = n.value().map(|v| self.visit(&v));
                self.consume_pow_before(node_span(node).end);
                let operator = self.op(span(&n.operator_loc()));
                let loc = self.to(operator, value.unwrap_or(operator));
                self.add(Kind::AssocSplat { value }, loc)
            }
            pm::Node::BackReferenceReadNode { .. } | pm::Node::NumberedReferenceReadNode { .. } => {
                self.token(node_span(node), |value| Kind::Backref { value })
            }
            pm::Node::BeginNode { .. } => self.begin(node),
            pm::Node::BlockNode { .. } => {
                let n = node.as_block_node().unwrap();
                self.block(&n)
            }
            pm::Node::BlockArgumentNode { .. } => {
                // Only reached outside of an argument list.
                let n = node.as_block_argument_node().unwrap();
                self.arg_block(&n)
            }
            pm::Node::BreakNode { .. } => {
                let n = node.as_break_node().unwrap();
                let arguments = self.flow_arguments(n.arguments(), node_span(node));
                let keyword = self.kw(span(&n.keyword_loc()));
                let loc = self.flow_location(keyword, arguments);
                self.add(Kind::Break { arguments }, loc)
            }
            pm::Node::NextNode { .. } => {
                let n = node.as_next_node().unwrap();
                let arguments = self.flow_arguments(n.arguments(), node_span(node));
                let keyword = self.kw(span(&n.keyword_loc()));
                let loc = self.flow_location(keyword, arguments);
                self.add(Kind::Next { arguments }, loc)
            }
            pm::Node::ReturnNode { .. } => {
                let n = node.as_return_node().unwrap();
                let keyword = self.kw(span(&n.keyword_loc()));
                match n.arguments() {
                    None => {
                        let loc = self.loc(keyword);
                        self.add(Kind::ReturnNode { arguments: None }, loc)
                    }
                    Some(args) => {
                        let arguments = self.arguments_list(
                            &args.arguments().iter().collect::<Vec<_>>(),
                            node_span(node).start,
                        );
                        let loc = self.to(keyword, arguments);
                        self.add(
                            Kind::ReturnNode {
                                arguments: Some(arguments),
                            },
                            loc,
                        )
                    }
                }
            }
            pm::Node::CallNode { .. } => self.call(node),
            pm::Node::CallAndWriteNode { .. }
            | pm::Node::CallOrWriteNode { .. }
            | pm::Node::CallOperatorWriteNode { .. } => self.call_op_write(node),
            pm::Node::CallTargetNode { .. } => {
                let n = node.as_call_target_node().unwrap();
                let receiver = self.visit(&n.receiver());
                let message_span = span(&n.message_loc());
                if self.slice(span(&n.call_operator_loc())) == "::" {
                    let constant = self.visit_token(message_span, true);
                    self.const_path_field(receiver, constant, span(&n.call_operator_loc()))
                } else {
                    let operator = self.visit_token(span(&n.call_operator_loc()), true);
                    let name = self.visit_token(message_span, true);
                    let loc = self.to(receiver, name);
                    self.add(
                        Kind::Field {
                            parent: receiver,
                            operator: CallOperator::Node(operator),
                            name,
                        },
                        loc,
                    )
                }
            }
            pm::Node::CapturePatternNode { .. } => {
                let n = node.as_capture_pattern_node().unwrap();
                let left = self.visit_pattern(&n.value());
                let right = self.visit(&n.target().as_node());
                self.binary(left, "=>", right)
            }
            pm::Node::CaseNode { .. } => self.case(node),
            pm::Node::CaseMatchNode { .. } => self.case_match(node),
            pm::Node::ClassNode { .. } => self.class(node),
            pm::Node::ClassVariableReadNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::CVar { value });
                self.var_ref(value)
            }
            pm::Node::ClassVariableWriteNode { .. } => {
                let n = node.as_class_variable_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::CVar { value });
                self.assign_var(target, &n.value())
            }
            pm::Node::ClassVariableOperatorWriteNode { .. } => {
                let n = node.as_class_variable_operator_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::CVar { value });
                self.op_assign_var(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::ClassVariableAndWriteNode { .. } => {
                let n = node.as_class_variable_and_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::CVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::ClassVariableOrWriteNode { .. } => {
                let n = node.as_class_variable_or_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::CVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::ClassVariableTargetNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::CVar { value });
                self.var_field(value)
            }
            pm::Node::ConstantReadNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::Const { value });
                self.var_ref(value)
            }
            pm::Node::ConstantWriteNode { .. } => {
                let n = node.as_constant_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::Const { value });
                self.assign_var(target, &n.value())
            }
            pm::Node::ConstantOperatorWriteNode { .. } => {
                let n = node.as_constant_operator_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::Const { value });
                self.op_assign_var(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::ConstantAndWriteNode { .. } => {
                let n = node.as_constant_and_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::Const { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::ConstantOrWriteNode { .. } => {
                let n = node.as_constant_or_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::Const { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::ConstantTargetNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::Const { value });
                self.var_field(value)
            }
            pm::Node::ConstantPathNode { .. } => {
                let n = node.as_constant_path_node().unwrap();
                match n.parent() {
                    None => {
                        let constant =
                            self.token(span(&n.name_loc()), |value| Kind::Const { value });
                        let colon = self.op(span(&n.delimiter_loc()));
                        let loc = self.to(colon, constant);
                        self.add(Kind::TopConstRef { constant }, loc)
                    }
                    Some(parent) => {
                        let parent = self.visit(&parent);
                        let constant =
                            self.token(span(&n.name_loc()), |value| Kind::Const { value });
                        let loc = self.to(parent, constant);
                        self.add(Kind::ConstPathRef { parent, constant }, loc)
                    }
                }
            }
            pm::Node::ConstantPathWriteNode { .. } => {
                let n = node.as_constant_path_write_node().unwrap();
                let target = self.const_path_target(&n.target().as_node());
                let value = self.visit_write_value(&n.value());
                let loc = self.to(target, value);
                self.add(Kind::Assign { target, value }, loc)
            }
            pm::Node::ConstantPathOperatorWriteNode { .. } => {
                let n = node.as_constant_path_operator_write_node().unwrap();
                let target = self.const_path_target(&n.target().as_node());
                self.op_assign(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::ConstantPathAndWriteNode { .. } => {
                let n = node.as_constant_path_and_write_node().unwrap();
                let target = self.const_path_target(&n.target().as_node());
                self.op_assign(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::ConstantPathOrWriteNode { .. } => {
                let n = node.as_constant_path_or_write_node().unwrap();
                let target = self.const_path_target(&n.target().as_node());
                self.op_assign(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::ConstantPathTargetNode { .. } => self.const_path_target(node),
            pm::Node::DefNode { .. } => self.def(node),
            pm::Node::DefinedNode { .. } => {
                let n = node.as_defined_node().unwrap();
                let value = self.visit(&n.value());
                let keyword = self.kw(span(&n.keyword_loc()));
                let between = Span {
                    start: self.loc(keyword).end_char,
                    end: self.loc(value).start_char,
                };
                let ending = if between.start <= between.end && self.slice(between).contains('(') {
                    match n.rparen_loc() {
                        Some(r) => self.token(span(&r), |value| Kind::Op { value }),
                        None => value,
                    }
                } else {
                    value
                };
                let loc = self.to(keyword, ending);
                self.add(Kind::Defined { value }, loc)
            }
            pm::Node::FalseNode { .. }
            | pm::Node::NilNode { .. }
            | pm::Node::TrueNode { .. }
            | pm::Node::SelfNode { .. }
            | pm::Node::SourceEncodingNode { .. }
            | pm::Node::SourceFileNode { .. }
            | pm::Node::SourceLineNode { .. } => {
                let kw = self.kw(node_span(node));
                self.var_ref(kw)
            }
            pm::Node::FindPatternNode { .. } => self.find_pattern(node),
            pm::Node::FlipFlopNode { .. } => {
                let n = node.as_flip_flop_node().unwrap();
                let left = n.left().map(|l| self.visit(&l));
                let right = n.right().map(|r| self.visit(&r));
                self.range(left, span(&n.operator_loc()), right)
            }
            pm::Node::RangeNode { .. } => {
                let n = node.as_range_node().unwrap();
                let left = n.left().map(|l| self.visit(&l));
                let right = n.right().map(|r| self.visit(&r));
                self.range(left, span(&n.operator_loc()), right)
            }
            pm::Node::FloatNode { .. } => {
                self.number(node_span(node), |value| Kind::FloatLiteral { value })
            }
            pm::Node::IntegerNode { .. } => {
                self.number(node_span(node), |value| Kind::Int { value })
            }
            pm::Node::RationalNode { .. } => {
                self.number(node_span(node), |value| Kind::RationalLiteral { value })
            }
            pm::Node::ImaginaryNode { .. } => {
                self.number(node_span(node), |value| Kind::Imaginary { value })
            }
            pm::Node::ForNode { .. } => self.for_loop(node),
            pm::Node::ForwardingArgumentsNode { .. } | pm::Node::ForwardingParameterNode { .. } => {
                let loc = self.token_loc(node_span(node));
                self.add(Kind::ArgsForward, loc)
            }
            pm::Node::ForwardingSuperNode { .. } => {
                let n = node.as_forwarding_super_node().unwrap();
                let s = node_span(node);
                let zsuper_span = Span {
                    start: s.start,
                    end: s.start + 5,
                };
                let loc = self.token_loc(zsuper_span);
                let zsuper = self.add(Kind::ZSuper, loc);
                match n.block() {
                    None => zsuper,
                    Some(block) => {
                        let block = self.visit(&block.as_node());
                        self.method_add_block(zsuper, block)
                    }
                }
            }
            pm::Node::GlobalVariableReadNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::GVar { value });
                self.var_ref(value)
            }
            pm::Node::GlobalVariableWriteNode { .. } => {
                let n = node.as_global_variable_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::GVar { value });
                self.assign_var(target, &n.value())
            }
            pm::Node::GlobalVariableOperatorWriteNode { .. } => {
                let n = node.as_global_variable_operator_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::GVar { value });
                self.op_assign_var(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::GlobalVariableAndWriteNode { .. } => {
                let n = node.as_global_variable_and_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::GVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::GlobalVariableOrWriteNode { .. } => {
                let n = node.as_global_variable_or_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::GVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::GlobalVariableTargetNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::GVar { value });
                self.var_field(value)
            }
            pm::Node::HashNode { .. } => {
                let n = node.as_hash_node().unwrap();
                let assocs: Vec<NodeId> = n.elements().iter().map(|e| self.visit(&e)).collect();
                let lbrace = self.token(span(&n.opening_loc()), |value| Kind::LBrace { value });
                let rbrace_loc = self.token_loc(span(&n.closing_loc()));
                let loc = self.loc(lbrace).to(&rbrace_loc);
                self.add(Kind::HashLiteral { lbrace, assocs }, loc)
            }
            pm::Node::HashPatternNode { .. } => self.hash_pattern(node),
            pm::Node::IfNode { .. } => self.if_node(node, None),
            pm::Node::UnlessNode { .. } => self.unless_node(node),
            pm::Node::IndexOperatorWriteNode { .. }
            | pm::Node::IndexAndWriteNode { .. }
            | pm::Node::IndexOrWriteNode { .. } => self.index_op_write(node),
            pm::Node::IndexTargetNode { .. } => {
                let n = node.as_index_target_node().unwrap();
                let receiver = self.visit(&n.receiver());
                let args =
                    self.call_arguments(n.arguments(), n.block().map(|b| b.as_node()), false);
                let rbracket = self.token_loc(span(&n.closing_loc()));
                let loc = self.loc(receiver).to(&rbracket);
                self.add(
                    Kind::ARefField {
                        collection: receiver,
                        index: args.arguments,
                    },
                    loc,
                )
            }
            pm::Node::InstanceVariableReadNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::IVar { value });
                self.var_ref(value)
            }
            pm::Node::InstanceVariableWriteNode { .. } => {
                let n = node.as_instance_variable_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::IVar { value });
                self.assign_var(target, &n.value())
            }
            pm::Node::InstanceVariableOperatorWriteNode { .. } => {
                let n = node.as_instance_variable_operator_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::IVar { value });
                self.op_assign_var(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::InstanceVariableAndWriteNode { .. } => {
                let n = node.as_instance_variable_and_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::IVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::InstanceVariableOrWriteNode { .. } => {
                let n = node.as_instance_variable_or_write_node().unwrap();
                let target = self.token(span(&n.name_loc()), |value| Kind::IVar { value });
                self.op_assign_var(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::InstanceVariableTargetNode { .. } => {
                let value = self.token(node_span(node), |value| Kind::IVar { value });
                self.var_field(value)
            }
            pm::Node::InterpolatedMatchLastLineNode { .. } => {
                let n = node.as_interpolated_match_last_line_node().unwrap();
                let parts: Vec<NodeId> = n.parts().iter().map(|p| self.string_part(&p)).collect();
                self.regexp(span(&n.opening_loc()), parts, span(&n.closing_loc()))
            }
            pm::Node::InterpolatedRegularExpressionNode { .. } => {
                let n = node.as_interpolated_regular_expression_node().unwrap();
                let parts: Vec<NodeId> = n.parts().iter().map(|p| self.string_part(&p)).collect();
                self.regexp(span(&n.opening_loc()), parts, span(&n.closing_loc()))
            }
            pm::Node::MatchLastLineNode { .. } => {
                let n = node.as_match_last_line_node().unwrap();
                let content = span(&n.content_loc());
                let parts = if content.start == content.end {
                    vec![]
                } else {
                    vec![self.tstring_content(content)]
                };
                self.regexp(span(&n.opening_loc()), parts, span(&n.closing_loc()))
            }
            pm::Node::RegularExpressionNode { .. } => {
                let n = node.as_regular_expression_node().unwrap();
                let content = span(&n.content_loc());
                let parts = if content.start == content.end {
                    vec![]
                } else {
                    vec![self.tstring_content(content)]
                };
                self.regexp(span(&n.opening_loc()), parts, span(&n.closing_loc()))
            }
            pm::Node::InterpolatedStringNode { .. } => self.interpolated_string(node),
            pm::Node::StringNode { .. } => self.string(node),
            pm::Node::XStringNode { .. } | pm::Node::InterpolatedXStringNode { .. } => {
                self.xstring(node)
            }
            pm::Node::InterpolatedSymbolNode { .. } => {
                let n = node.as_interpolated_symbol_node().unwrap();
                let parts: Vec<NodeId> = n.parts().iter().map(|p| self.string_part(&p)).collect();
                self.dyna_symbol(
                    n.opening_loc().map(|l| span(&l)),
                    parts,
                    n.closing_loc().map(|l| span(&l)),
                    node_span(node),
                )
            }
            pm::Node::SymbolNode { .. } => self.symbol(node),
            pm::Node::ItLocalVariableReadNode { .. } => {
                let ident = self.ident(node_span(node));
                let loc = self.loc(ident);
                self.add(Kind::VCall { value: ident }, loc)
            }
            pm::Node::KeywordHashNode { .. } => {
                let n = node.as_keyword_hash_node().unwrap();
                let assocs: Vec<NodeId> = n.elements().iter().map(|e| self.visit(&e)).collect();
                let loc = self.to(assocs[0], *assocs.last().unwrap());
                self.add(Kind::BareAssocHash { assocs }, loc)
            }
            pm::Node::LambdaNode { .. } => self.lambda(node),
            pm::Node::LocalVariableReadNode { .. } => {
                let value = self.ident(node_span(node));
                self.var_ref(value)
            }
            pm::Node::LocalVariableWriteNode { .. } => {
                let n = node.as_local_variable_write_node().unwrap();
                let target = self.ident(span(&n.name_loc()));
                self.assign_var(target, &n.value())
            }
            pm::Node::LocalVariableOperatorWriteNode { .. } => {
                let n = node.as_local_variable_operator_write_node().unwrap();
                let target = self.ident(span(&n.name_loc()));
                self.op_assign_var(target, span(&n.binary_operator_loc()), None, &n.value())
            }
            pm::Node::LocalVariableAndWriteNode { .. } => {
                let n = node.as_local_variable_and_write_node().unwrap();
                let target = self.ident(span(&n.name_loc()));
                self.op_assign_var(target, span(&n.operator_loc()), Some("&&="), &n.value())
            }
            pm::Node::LocalVariableOrWriteNode { .. } => {
                let n = node.as_local_variable_or_write_node().unwrap();
                let target = self.ident(span(&n.name_loc()));
                self.op_assign_var(target, span(&n.operator_loc()), Some("||="), &n.value())
            }
            pm::Node::LocalVariableTargetNode { .. } => {
                let value = self.ident(node_span(node));
                self.var_field(value)
            }
            pm::Node::MatchPredicateNode { .. } => {
                let n = node.as_match_predicate_node().unwrap();
                let value = self.visit(&n.value());
                let pattern = self.visit_pattern(&n.pattern());
                let operator = self.kw(span(&n.keyword_loc()));
                let loc = self.to(value, pattern);
                self.add(
                    Kind::RAssign {
                        value,
                        operator,
                        pattern,
                    },
                    loc,
                )
            }
            pm::Node::MatchRequiredNode { .. } => {
                let n = node.as_match_required_node().unwrap();
                let value = self.visit(&n.value());
                let pattern = self.visit_pattern(&n.pattern());
                let operator = self.op(span(&n.operator_loc()));
                let loc = self.to(value, pattern);
                self.add(
                    Kind::RAssign {
                        value,
                        operator,
                        pattern,
                    },
                    loc,
                )
            }
            pm::Node::MatchWriteNode { .. } => {
                let n = node.as_match_write_node().unwrap();
                self.visit(&n.call().as_node())
            }
            pm::Node::ModuleNode { .. } => self.module(node),
            pm::Node::MultiTargetNode { .. } => {
                let n = node.as_multi_target_node().unwrap();
                let lefts: Vec<pm::Node> = n.lefts().iter().collect();
                let rights: Vec<pm::Node> = n.rights().iter().collect();
                let targets = self.mlhs_targets(&lefts, n.rest(), &rights, true);
                match (n.lparen_loc(), n.rparen_loc()) {
                    (Some(l), Some(r)) => self.mlhs_paren(targets, span(&l), span(&r)),
                    _ => targets,
                }
            }
            pm::Node::MultiWriteNode { .. } => self.multi_write(node),
            pm::Node::ParenthesesNode { .. } => {
                let n = node.as_parentheses_node().unwrap();
                let lparen = self.token(span(&n.opening_loc()), |value| Kind::LParen { value });
                let contents = match n.body() {
                    None => {
                        let void = self.void_stmt(span(&n.closing_loc()).start);
                        let loc = self.loc(void);
                        self.add(Kind::Statements { body: vec![void] }, loc)
                    }
                    Some(body) => match body.as_statements_node() {
                        Some(stmts) => {
                            let body: Vec<NodeId> =
                                stmts.body().iter().map(|s| self.visit(&s)).collect();
                            self.statements(body, span(&n.closing_loc()).start)
                        }
                        None => {
                            let inner = self.visit(&body);
                            let loc = self.loc(inner);
                            self.add(Kind::Statements { body: vec![inner] }, loc)
                        }
                    },
                };
                let rparen = self.token_loc(span(&n.closing_loc()));
                let loc = self.loc(lparen).to(&rparen);
                self.add(
                    Kind::Paren {
                        lparen,
                        contents: Some(contents),
                    },
                    loc,
                )
            }
            pm::Node::PinnedExpressionNode { .. } => {
                let n = node.as_pinned_expression_node().unwrap();
                let statement = self.visit(&n.expression());
                let pin = self.token_loc(span(&n.operator_loc()));
                let rparen = self.token_loc(span(&n.rparen_loc()));
                let loc = pin.to(&rparen);
                self.add(Kind::PinnedBegin { statement }, loc)
            }
            pm::Node::PinnedVariableNode { .. } => {
                let n = node.as_pinned_variable_node().unwrap();
                let var_ref = self.visit(&n.variable());
                let value = match self.kind(var_ref) {
                    Kind::VarRef { value } => *value,
                    _ => var_ref,
                };
                let pin = self.token_loc(span(&n.operator_loc()));
                let loc = pin.to(&self.loc(value));
                self.add(Kind::PinnedVarRef { value }, loc)
            }
            pm::Node::PostExecutionNode { .. } => {
                let n = node.as_post_execution_node().unwrap();
                let (lbrace, statements, loc) = self.pre_post_execution(
                    n.statements(),
                    span(&n.keyword_loc()),
                    span(&n.opening_loc()),
                    span(&n.closing_loc()),
                );
                self.add(Kind::ENDBlock { lbrace, statements }, loc)
            }
            pm::Node::PreExecutionNode { .. } => {
                let n = node.as_pre_execution_node().unwrap();
                let (lbrace, statements, loc) = self.pre_post_execution(
                    n.statements(),
                    span(&n.keyword_loc()),
                    span(&n.opening_loc()),
                    span(&n.closing_loc()),
                );
                self.add(Kind::BEGINBlock { lbrace, statements }, loc)
            }
            pm::Node::RequiredParameterNode { .. } => self.ident(node_span(node)),
            pm::Node::RedoNode { .. } => {
                let loc = self.token_loc(node_span(node));
                self.add(Kind::Redo, loc)
            }
            pm::Node::RetryNode { .. } => {
                let loc = self.token_loc(node_span(node));
                self.add(Kind::Retry, loc)
            }
            pm::Node::RescueModifierNode { .. } => {
                let n = node.as_rescue_modifier_node().unwrap();
                let statement = self.visit_write_value(&n.expression());
                let value = self.visit(&n.rescue_expression());
                let loc = self.to(statement, value);
                self.add(Kind::RescueMod { statement, value }, loc)
            }
            pm::Node::ShareableConstantNode { .. } => {
                let n = node.as_shareable_constant_node().unwrap();
                self.visit(&n.write())
            }
            pm::Node::SingletonClassNode { .. } => self.sclass(node),
            pm::Node::SplatNode { .. } => {
                let n = node.as_splat_node().unwrap();
                self.arg_star(&n)
            }
            pm::Node::StatementsNode { .. } => {
                let n = node.as_statements_node().unwrap();
                let body: Vec<NodeId> = n.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, node_span(node).start)
            }
            pm::Node::SuperNode { .. } => self.super_node(node),
            pm::Node::UndefNode { .. } => {
                let n = node.as_undef_node().unwrap();
                let symbols: Vec<NodeId> = n.names().iter().map(|s| self.visit(&s)).collect();
                let keyword = self.kw(span(&n.keyword_loc()));
                let loc = self.to(keyword, *symbols.last().unwrap());
                self.add(Kind::Undef { symbols }, loc)
            }
            pm::Node::UntilNode { .. } => self.loop_node(node, false),
            pm::Node::WhileNode { .. } => self.loop_node(node, true),
            pm::Node::YieldNode { .. } => self.yield_node(node),
            pm::Node::EmbeddedStatementsNode { .. } | pm::Node::EmbeddedVariableNode { .. } => {
                self.string_part(node)
            }
            pm::Node::ImplicitNode { .. } => {
                let n = node.as_implicit_node().unwrap();
                self.visit(&n.value())
            }
            pm::Node::NumberedParametersNode { .. }
            | pm::Node::ItParametersNode { .. }
            | pm::Node::ErrorRecoveryNode { .. } => {
                let loc = self.fixed_loc(node_span(node).start);
                self.add(Kind::VoidStmt, loc)
            }
            _ => {
                let loc = self.fixed_loc(node_span(node).start);
                self.add(Kind::VoidStmt, loc)
            }
        }
    }

    fn var_ref(&mut self, value: NodeId) -> NodeId {
        let loc = self.loc(value);
        self.add(Kind::VarRef { value }, loc)
    }

    fn var_field(&mut self, value: NodeId) -> NodeId {
        let loc = self.loc(value);
        self.add(
            Kind::VarField {
                value: NilOr::Node(value),
            },
            loc,
        )
    }

    fn alias_gvar(&mut self, node: &pm::Node<'_>) -> NodeId {
        match node {
            pm::Node::BackReferenceReadNode { .. } | pm::Node::NumberedReferenceReadNode { .. } => {
                self.token(node_span(node), |value| Kind::Backref { value })
            }
            _ => self.token(node_span(node), |value| Kind::GVar { value }),
        }
    }

    fn binary(&mut self, left: NodeId, operator: &str, right: NodeId) -> NodeId {
        let loc = self.to(left, right);
        self.add(
            Kind::Binary {
                left,
                operator: operator.to_string(),
                right,
            },
            loc,
        )
    }

    fn range(&mut self, left: Option<NodeId>, op: Span, right: Option<NodeId>) -> NodeId {
        let operator = self.op(op);
        let beginning = left.unwrap_or(operator);
        let ending = right.unwrap_or(operator);
        let loc = self.to(beginning, ending);
        self.add(
            Kind::RangeNode {
                left,
                operator,
                right,
            },
            loc,
        )
    }

    /// `visit_number_node`: a leading `-` becomes a unary minus.
    fn number(&mut self, s: Span, make: fn(String) -> Kind) -> NodeId {
        let slice = self.slice(s);
        if let Some(rest) = slice.strip_prefix('-') {
            let inner_span = Span {
                start: s.start + 1,
                end: s.end,
            };
            let loc = self.token_loc(inner_span);
            let value = self.add(make(rest.to_string()), loc);
            let operator_loc = self.token_loc(Span {
                start: s.start,
                end: s.start + 1,
            });
            let loc = operator_loc.to(&self.loc(value));
            self.add(
                Kind::Unary {
                    operator: "-".to_string(),
                    statement: value,
                },
                loc,
            )
        } else {
            self.token(s, make)
        }
    }

    fn assign_var(&mut self, target_value: NodeId, value: &pm::Node<'_>) -> NodeId {
        let target = self.var_field(target_value);
        let value = self.visit_write_value(value);
        let loc = self.to(target, value);
        self.add(Kind::Assign { target, value }, loc)
    }

    fn op_assign_var(
        &mut self,
        target_value: NodeId,
        op: Span,
        op_text: Option<&str>,
        value: &pm::Node<'_>,
    ) -> NodeId {
        let target = self.var_field(target_value);
        self.op_assign(target, op, op_text, value)
    }

    fn op_assign(
        &mut self,
        target: NodeId,
        op: Span,
        op_text: Option<&str>,
        value: &pm::Node<'_>,
    ) -> NodeId {
        let operator = match op_text {
            Some(text) => self.op_value(op, text),
            None => self.op(op),
        };
        let value = self.visit_write_value(value);
        let loc = self.to(target, value);
        self.add(
            Kind::OpAssign {
                target,
                operator,
                value,
            },
            loc,
        )
    }

    fn const_path_target(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node
            .as_constant_path_target_node()
            .map(|n| (n.parent(), span(&n.name_loc()), span(&n.delimiter_loc())));
        let (parent, name, delimiter) = match n {
            Some(parts) => parts,
            None => {
                let p = node.as_constant_path_node().unwrap();
                (p.parent(), span(&p.name_loc()), span(&p.delimiter_loc()))
            }
        };
        match parent {
            None => {
                let constant = self.token(name, |value| Kind::Const { value });
                let colon = self.token_loc(delimiter);
                let loc = colon.to(&self.loc(constant));
                self.add(Kind::TopConstField { constant }, loc)
            }
            Some(parent) => {
                let parent = self.visit(&parent);
                let constant = self.token(name, |value| Kind::Const { value });
                self.const_path_field(parent, constant, delimiter)
            }
        }
    }

    /// `on_const_path_field`
    fn const_path_field(&mut self, parent: NodeId, constant: NodeId, delimiter: Span) -> NodeId {
        let loc = self.to(parent, constant);
        if matches!(self.kind(constant), Kind::Const { .. }) {
            self.add(Kind::ConstPathField { parent, constant }, loc)
        } else {
            let operator = self.op(delimiter);
            self.add(
                Kind::Field {
                    parent,
                    operator: CallOperator::Node(operator),
                    name: constant,
                },
                loc,
            )
        }
    }

    /// `visit_write_value`: a bracketless array on the right of an
    /// assignment becomes an `MRHS`.
    fn visit_write_value(&mut self, node: &pm::Node<'_>) -> NodeId {
        if let Some(array) = node.as_array_node()
            && array.opening_loc().is_none()
        {
            let elements: Vec<pm::Node> = array.elements().iter().collect();
            let length = elements.len();
            let first_is_lone_splat = length == 1 && elements[0].as_splat_node().is_some();
            // `args` holds the parts so far; `mrhs_location` mirrors Ripper's
            // running location.
            let mut parts: Vec<NodeId> = Vec::new();
            let mut location: Option<Location> = None;
            let fixed = self.fixed_loc(node_span(&elements[0]).start);
            for (index, element) in elements.iter().enumerate() {
                if index == length - 1 {
                    if let Some(splat) = element.as_splat_node() {
                        let star = self.arg_star(&splat);
                        let star_loc = self.loc(star);
                        location = Some(if parts.is_empty() {
                            star_loc
                        } else {
                            location.unwrap_or(fixed).to(&star_loc)
                        });
                        parts.push(star);
                    } else {
                        let arg = self.visit(element);
                        let mrhs_loc = location.unwrap_or(fixed);
                        location = Some(if parts.is_empty() {
                            mrhs_loc
                        } else {
                            mrhs_loc.to(&self.loc(arg))
                        });
                        parts.push(arg);
                    }
                } else if let Some(splat) = element.as_splat_node() {
                    let star = self.arg_star(&splat);
                    // on_args_add_star: the location of the first part is
                    // its value, not the star.
                    let value_loc = match self.kind(star) {
                        Kind::ArgStar { value: Some(v) } => self.loc(*v),
                        _ => self.loc(star),
                    };
                    location = Some(if parts.is_empty() {
                        value_loc
                    } else {
                        location.unwrap_or(fixed).to(&value_loc)
                    });
                    parts.push(star);
                } else {
                    let arg = self.visit(element);
                    let arg_loc = self.loc(arg);
                    location = Some(if parts.is_empty() {
                        arg_loc
                    } else {
                        location.unwrap_or(fixed).to(&arg_loc)
                    });
                    parts.push(arg);
                }
            }
            let _ = first_is_lone_splat;
            let loc = location.unwrap_or(fixed);
            return self.add(Kind::MRHS { parts }, loc);
        }
        self.visit(node)
    }

    // ------------------------------------------------------------------
    // Arguments
    // ------------------------------------------------------------------

    fn arg_star(&mut self, splat: &pm::SplatNode<'_>) -> NodeId {
        let value = splat.expression().map(|e| self.visit(&e));
        let star = self.token_loc(span(&splat.operator_loc()));
        let loc = match value {
            Some(v) => star.to(&self.loc(v)),
            None => star,
        };
        self.add(Kind::ArgStar { value }, loc)
    }

    fn arg_block(&mut self, block: &pm::BlockArgumentNode<'_>) -> NodeId {
        let value = block.expression().map(|e| self.visit(&e));
        let operator = self.token_loc(span(&block.operator_loc()));
        let loc = match value {
            Some(v) => operator.to(&self.loc(v)),
            None => operator,
        };
        self.add(Kind::ArgBlock { value }, loc)
    }

    /// `visit_arguments` + the `on_args_*` handlers: builds an `Args` node.
    fn arguments_list(&mut self, elements: &[pm::Node<'_>], fallback: usize) -> NodeId {
        let mut parts: Vec<NodeId> = Vec::new();
        let mut location: Option<Location> = None;
        for element in elements {
            match element {
                pm::Node::BlockArgumentNode { .. } => {
                    // on_args_add_block gives up when a token it scans sits
                    // before the end of the arguments so far, which happens
                    // when a heredoc extends them: syntax_tree drops `&blk`.
                    // This changes the program, and is kept on purpose:
                    // oxtree keeps byte-parity with syntax_tree 6.3.0, bugs
                    // included (user decision, see docs/OXTREE.md).
                    if let Some(l) = location
                        && !parts.is_empty()
                        && l.end_char > node_span(element).end
                    {
                        continue;
                    }
                    let arg = self.arg_block(&element.as_block_argument_node().unwrap());
                    let arg_loc = self.loc(arg);
                    location = Some(match location {
                        Some(l) => l.to(&arg_loc),
                        None => self.lookahead_loc(arg_loc.end_char).to(&arg_loc),
                    });
                    parts.push(arg);
                }
                pm::Node::SplatNode { .. } => {
                    let star = self.arg_star(&element.as_splat_node().unwrap());
                    let ending = match self.kind(star) {
                        Kind::ArgStar { value: Some(v) } => self.loc(*v),
                        _ => self.token_loc(span(&element.as_splat_node().unwrap().operator_loc())),
                    };
                    location = Some(match location {
                        Some(l) if !parts.is_empty() => l.to(&ending),
                        _ => ending,
                    });
                    parts.push(star);
                }
                _ => {
                    let arg = self.visit(element);
                    let arg_loc = self.loc(arg);
                    location = Some(match location {
                        Some(l) if !parts.is_empty() => l.to(&arg_loc),
                        _ => arg_loc,
                    });
                    parts.push(arg);
                }
            }
        }
        let loc = location.unwrap_or_else(|| self.fixed_loc(fallback));
        self.add(Kind::Args { parts }, loc)
    }

    /// `visit_call_node_arguments`
    fn call_arguments(
        &mut self,
        arguments: Option<pm::ArgumentsNode<'_>>,
        block: Option<pm::Node<'_>>,
        _trailing_comma: bool,
    ) -> CallArguments {
        let mut elements: Vec<pm::Node> = arguments
            .as_ref()
            .map(|a| a.arguments().iter().collect())
            .unwrap_or_default();
        let mut ripper_block = None;
        if let Some(block) = block {
            if block.as_block_argument_node().is_some() {
                elements.push(block);
            } else {
                ripper_block = Some(block);
            }
        }
        let args = if elements.len() == 1 && elements[0].as_forwarding_arguments_node().is_some() {
            Some(self.visit(&elements[0]))
        } else if !elements.is_empty() {
            let fallback = node_span(&elements[0]).start;
            Some(self.arguments_list(&elements, fallback))
        } else {
            None
        };
        let block = ripper_block.map(|b| self.visit(&b));
        CallArguments {
            arguments: args,
            block,
        }
    }

    fn flow_arguments(&mut self, arguments: Option<pm::ArgumentsNode<'_>>, s: Span) -> NodeId {
        match arguments {
            None => {
                let loc = self.lookahead_loc(s.end);
                self.add(Kind::Args { parts: vec![] }, loc)
            }
            Some(args) => {
                let elements: Vec<pm::Node> = args.arguments().iter().collect();
                self.arguments_list(&elements, s.start)
            }
        }
    }

    fn flow_location(&self, keyword: NodeId, arguments: NodeId) -> Location {
        match self.kind(arguments) {
            Kind::Args { parts } if !parts.is_empty() => self.to(keyword, arguments),
            _ => self.loc(keyword),
        }
    }

    /// `on_arg_paren`
    fn arg_paren(&mut self, arguments: Option<NodeId>, lparen: Span, rparen: Span) -> NodeId {
        let lparen_loc = self.token_loc(lparen);
        let rparen_loc = self.token_loc(rparen);
        let ending = match arguments {
            Some(args) if self.loc(args).end_line > rparen_loc.end_line => self.loc(args),
            _ => rparen_loc,
        };
        let loc = lparen_loc.to(&ending);
        self.add(Kind::ArgParen { arguments }, loc)
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    fn call(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_call_node().unwrap();
        let name = n.name().as_slice();
        let name = std::str::from_utf8(name).unwrap_or("");

        if n.call_operator_loc().is_none() {
            match name {
                "[]" if n.receiver().is_some() && n.opening_loc().is_some() => {
                    let receiver = self.visit(&n.receiver().unwrap());
                    let args = self.call_arguments(n.arguments(), n.block(), false);
                    let rbracket = self.token_loc(span(&n.closing_loc().unwrap()));
                    let loc = self.loc(receiver).to(&rbracket);
                    let call = self.add(
                        Kind::ARef {
                            collection: receiver,
                            index: args.arguments,
                        },
                        loc,
                    );
                    return match args.block {
                        Some(block) => self.method_add_block(call, block),
                        None => call,
                    };
                }
                "[]=" if n.receiver().is_some() && n.opening_loc().is_some() => {
                    let receiver = self.visit(&n.receiver().unwrap());
                    let mut elements: Vec<pm::Node> = n
                        .arguments()
                        .map(|a| a.arguments().iter().collect())
                        .unwrap_or_default();
                    let last = elements.pop();
                    if let Some(block) = n.block() {
                        elements.push(block);
                    }
                    let index = if elements.is_empty() {
                        None
                    } else {
                        let fallback = node_span(&elements[0]).start;
                        Some(self.arguments_list(&elements, fallback))
                    };
                    let rbracket = self.token_loc(span(&n.closing_loc().unwrap()));
                    let loc = self.loc(receiver).to(&rbracket);
                    let target = self.add(
                        Kind::ARefField {
                            collection: receiver,
                            index,
                        },
                        loc,
                    );
                    let value = match last {
                        Some(last) => self.visit_write_value(&last),
                        None => target,
                    };
                    let loc = self.to(target, value);
                    return self.add(Kind::Assign { target, value }, loc);
                }
                "-@" | "+@" | "~" if n.receiver().is_some() => {
                    let receiver = self.visit(&n.receiver().unwrap());
                    let op = self.token_loc(span(&n.message_loc().unwrap()));
                    let loc = op.to(&self.loc(receiver));
                    return self.add(
                        Kind::Unary {
                            operator: name[..1].to_string(),
                            statement: receiver,
                        },
                        loc,
                    );
                }
                "!" if n.receiver().is_some() => {
                    let message = self.slice(span(&n.message_loc().unwrap()));
                    if message == "not" {
                        return self.not(&n);
                    }
                    let receiver = self.visit(&n.receiver().unwrap());
                    let op = self.token_loc(span(&n.message_loc().unwrap()));
                    let loc = op.to(&self.loc(receiver));
                    return self.add(
                        Kind::Unary {
                            operator: "!".to_string(),
                            statement: receiver,
                        },
                        loc,
                    );
                }
                _ if BINARY_OPERATORS.contains(&name)
                    && n.receiver().is_some()
                    && n.arguments().is_some_and(|a| a.arguments().len() == 1)
                    && n.opening_loc().is_none() =>
                {
                    let receiver = self.visit(&n.receiver().unwrap());
                    let value =
                        self.visit(&n.arguments().unwrap().arguments().iter().next().unwrap());
                    // on_binary deletes the operator token only if it starts
                    // after `left.end_char + 1`.
                    if name == "**" {
                        let pos = span(&n.message_loc().unwrap()).start;
                        if pos > self.loc(receiver).end_char && pos < self.loc(value).start_char {
                            if let Some(entry) = self.pow_tokens.iter_mut().find(|(p, _)| *p == pos)
                            {
                                entry.1 = true;
                            }
                        }
                    }
                    return self.binary(receiver, name, value);
                }
                _ => {}
            }

            let message_span = span(&n.message_loc().unwrap());
            let message = self.visit_token(message_span, false);
            if n.is_variable_call() {
                let loc = self.loc(message);
                return self.add(Kind::VCall { value: message }, loc);
            }
            let has_args = n.arguments().is_some()
                || n.block()
                    .is_some_and(|b| b.as_block_argument_node().is_some());
            let args = self.call_arguments(n.arguments(), n.block(), false);
            let call = if n.opening_loc().is_none() && has_args {
                let arguments = args.arguments.unwrap();
                let loc = self.to(message, arguments);
                self.add(
                    Kind::Command {
                        message,
                        arguments,
                        block: None,
                    },
                    loc,
                )
            } else if let Some(opening) = n.opening_loc() {
                let paren = self.arg_paren(
                    args.arguments,
                    span(&opening),
                    span(&n.closing_loc().unwrap()),
                );
                let loc = self.to(message, paren);
                self.add(
                    Kind::CallNode {
                        receiver: None,
                        operator: CallOperator::Nil,
                        message: Some(message),
                        arguments: Some(paren),
                    },
                    loc,
                )
            } else {
                // args_new is reduced after the block, if any.
                let empty_at = match args.block {
                    Some(block) => self.loc(block).end_char,
                    None => self.after_next_token(message_span.end),
                };
                let empty_loc = match args.block {
                    Some(_) => self.fixed_loc(empty_at),
                    None => self.lookahead_loc(message_span.end),
                };
                let empty = self.add(Kind::Args { parts: vec![] }, empty_loc);
                let loc = self.loc(message);
                self.add(
                    Kind::CallNode {
                        receiver: None,
                        operator: CallOperator::Nil,
                        message: Some(message),
                        arguments: Some(empty),
                    },
                    loc,
                )
            };
            return match args.block {
                Some(block) => self.method_add_block(call, block),
                None => call,
            };
        }

        // With a call operator.
        let receiver = self.visit(&n.receiver().unwrap());
        let operator_span = span(&n.call_operator_loc().unwrap());
        let call_operator = self.visit_token(operator_span, true);
        let message = n.message_loc().map(|m| self.visit_token(span(&m), false));
        let message_text = n.message_loc().map(|m| self.slice(span(&m))).unwrap_or("");

        if name.ends_with('=')
            && !message_text.ends_with('=')
            && n.arguments().is_some()
            && n.block().is_none()
            && let Some(name_node) = message
        {
            let loc = self.to(receiver, name_node);
            let field = self.add(
                Kind::Field {
                    parent: receiver,
                    operator: CallOperator::Node(call_operator),
                    name: name_node,
                },
                loc,
            );
            let value_node = n.arguments().unwrap().arguments().iter().next().unwrap();
            let value = self.visit_write_value(&value_node);
            let loc = self.to(field, value);
            return self.add(
                Kind::Assign {
                    target: field,
                    value,
                },
                loc,
            );
        }

        let has_block_arg = n
            .block()
            .is_some_and(|b| b.as_block_argument_node().is_some());
        // `a b do end.c d` is Ripper's block_command: a call whose arguments
        // are a bare Args, and whose location stops at the message.
        let block_command = n.opening_loc().is_none()
            && n.arguments().is_some()
            && matches!(
                self.kind(receiver),
                Kind::Command { block: Some(_), .. } | Kind::CommandCall { block: Some(_), .. }
            );
        let args = self.call_arguments(n.arguments(), n.block(), false);
        let call = if block_command {
            let ending = message.unwrap_or(call_operator);
            let loc = self.to(receiver, ending);
            self.add(
                Kind::CallNode {
                    receiver: Some(receiver),
                    operator: CallOperator::Node(call_operator),
                    message,
                    arguments: args.arguments,
                },
                loc,
            )
        } else if n.opening_loc().is_none() {
            if n.arguments().is_none() && !has_block_arg {
                let ending = message.unwrap_or(call_operator);
                let loc = self.to(receiver, ending);
                self.add(
                    Kind::CallNode {
                        receiver: Some(receiver),
                        operator: CallOperator::Node(call_operator),
                        message,
                        arguments: None,
                    },
                    loc,
                )
            } else {
                let ending = args.arguments.or(message).unwrap_or(call_operator);
                let loc = self.to(receiver, ending);
                self.add(
                    Kind::CommandCall {
                        receiver,
                        operator: CallOperator::Node(call_operator),
                        message,
                        arguments: args.arguments,
                        block: None,
                    },
                    loc,
                )
            }
        } else {
            let paren = self.arg_paren(
                args.arguments,
                span(&n.opening_loc().unwrap()),
                span(&n.closing_loc().unwrap()),
            );
            let ending = message.unwrap_or(call_operator);
            let call_loc = self.to(receiver, ending);
            let loc = call_loc.to(&self.loc(paren));
            self.add(
                Kind::CallNode {
                    receiver: Some(receiver),
                    operator: CallOperator::Node(call_operator),
                    message,
                    arguments: Some(paren),
                },
                loc,
            )
        };
        match args.block {
            Some(block) => self.method_add_block(call, block),
            None => call,
        }
    }

    fn not(&mut self, n: &pm::CallNode<'_>) -> NodeId {
        let keyword = self.kw(span(&n.message_loc().unwrap()));
        let keyword_end = self.loc(keyword).end_char;
        let parentheses = self.bytes.get(keyword_end) == Some(&b'(');
        let receiver = n.receiver().unwrap();
        let mut ending = None;
        let statement = if parentheses && let Some(paren) = receiver.as_parentheses_node() {
            ending = Some(self.token_loc(span(&paren.closing_loc())));
            match paren.body() {
                None => None,
                Some(body) => match body.as_statements_node() {
                    Some(stmts) => {
                        let first = stmts.body().iter().next();
                        first.map(|s| self.visit(&s))
                    }
                    None => Some(self.visit(&body)),
                },
            }
        } else {
            Some(self.visit(&receiver))
        };
        let keyword_loc = self.loc(keyword);
        let loc = match (ending, statement) {
            (Some(e), _) => keyword_loc.to(&e),
            (None, Some(s)) => keyword_loc.to(&self.loc(s)),
            (None, None) => keyword_loc,
        };
        self.add(
            Kind::Not {
                statement,
                parentheses,
            },
            loc,
        )
    }

    /// `on_method_add_block`
    fn method_add_block(&mut self, call: NodeId, block: NodeId) -> NodeId {
        let loc = self.to(call, block);
        match &mut self.nodes[call as usize].kind {
            Kind::Command { block: b, .. } | Kind::CommandCall { block: b, .. } => {
                *b = Some(block);
                self.nodes[call as usize].location = loc;
                call
            }
            _ => self.add(Kind::MethodAddBlock { call, block }, loc),
        }
    }

    fn call_op_write(&mut self, node: &pm::Node<'_>) -> NodeId {
        let (receiver, call_operator, message, op, op_text, value) =
            if let Some(n) = node.as_call_operator_write_node() {
                (
                    n.receiver(),
                    n.call_operator_loc(),
                    n.message_loc(),
                    span(&n.binary_operator_loc()),
                    None,
                    n.value(),
                )
            } else if let Some(n) = node.as_call_and_write_node() {
                (
                    n.receiver(),
                    n.call_operator_loc(),
                    n.message_loc(),
                    span(&n.operator_loc()),
                    Some("&&="),
                    n.value(),
                )
            } else {
                let n = node.as_call_or_write_node().unwrap();
                (
                    n.receiver(),
                    n.call_operator_loc(),
                    n.message_loc(),
                    span(&n.operator_loc()),
                    Some("||="),
                    n.value(),
                )
            };
        let receiver = self.visit(&receiver.unwrap());
        let operator = self.visit_token(span(&call_operator.unwrap()), true);
        let name = self.visit_token(span(&message.unwrap()), true);
        let loc = self.to(receiver, name);
        let target = self.add(
            Kind::Field {
                parent: receiver,
                operator: CallOperator::Node(operator),
                name,
            },
            loc,
        );
        self.op_assign(target, op, op_text, &value)
    }

    fn index_op_write(&mut self, node: &pm::Node<'_>) -> NodeId {
        let (receiver, arguments, block, closing, op, op_text, value) =
            if let Some(n) = node.as_index_operator_write_node() {
                (
                    n.receiver(),
                    n.arguments(),
                    n.block(),
                    span(&n.closing_loc()),
                    span(&n.binary_operator_loc()),
                    None,
                    n.value(),
                )
            } else if let Some(n) = node.as_index_and_write_node() {
                (
                    n.receiver(),
                    n.arguments(),
                    n.block(),
                    span(&n.closing_loc()),
                    span(&n.operator_loc()),
                    Some("&&="),
                    n.value(),
                )
            } else {
                let n = node.as_index_or_write_node().unwrap();
                (
                    n.receiver(),
                    n.arguments(),
                    n.block(),
                    span(&n.closing_loc()),
                    span(&n.operator_loc()),
                    Some("||="),
                    n.value(),
                )
            };
        let receiver = self.visit(&receiver.unwrap());
        let block = block.map(|b| b.as_node());
        let args = self.call_arguments(arguments, block, false);
        let rbracket = self.token_loc(closing);
        let loc = self.loc(receiver).to(&rbracket);
        let target = self.add(
            Kind::ARefField {
                collection: receiver,
                index: args.arguments,
            },
            loc,
        );
        self.op_assign(target, op, op_text, &value)
    }

    fn super_node(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_super_node().unwrap();
        let args = self.call_arguments(n.arguments(), n.block(), false);
        let keyword = self.kw(span(&n.keyword_loc()));
        let arguments = match n.lparen_loc() {
            Some(lparen) => self.arg_paren(
                args.arguments,
                span(&lparen),
                span(&n.rparen_loc().unwrap()),
            ),
            None => match args.arguments {
                Some(a) => a,
                None => {
                    let loc = self.fixed_loc(self.loc(keyword).end_char);
                    self.add(Kind::Args { parts: vec![] }, loc)
                }
            },
        };
        let loc = self.to(keyword, arguments);
        let call = self.add(Kind::Super { arguments }, loc);
        match args.block {
            Some(block) => self.method_add_block(call, block),
            None => call,
        }
    }

    fn yield_node(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_yield_node().unwrap();
        let keyword = self.kw(span(&n.keyword_loc()));
        if n.arguments().is_none() && n.lparen_loc().is_none() {
            let loc = self.loc(keyword);
            return self.add(Kind::YieldNode { arguments: None }, loc);
        }
        let mut arguments = match n.arguments() {
            Some(args) => {
                let elements: Vec<pm::Node> = args.arguments().iter().collect();
                Some(self.arguments_list(&elements, node_span(node).start))
            }
            None => None,
        };
        if let Some(lparen) = n.lparen_loc() {
            let lparen_node = self.token(span(&lparen), |value| Kind::LParen { value });
            let rparen = self.token_loc(span(&n.rparen_loc().unwrap()));
            let loc = self.loc(lparen_node).to(&rparen);
            arguments = Some(self.add(
                Kind::Paren {
                    lparen: lparen_node,
                    contents: arguments,
                },
                loc,
            ));
        }
        let arguments = arguments.unwrap();
        let loc = self.to(keyword, arguments);
        self.add(
            Kind::YieldNode {
                arguments: Some(arguments),
            },
            loc,
        )
    }

    // ------------------------------------------------------------------
    // Literals
    // ------------------------------------------------------------------

    fn array(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_array_node().unwrap();
        let opening = n.opening_loc().map(|l| span(&l));
        let opening_text = opening.map(|s| self.slice(s)).unwrap_or("");
        let closing = n.closing_loc().map(|l| span(&l));

        if opening_text.starts_with("%w") || opening_text.starts_with("%i") {
            let qwords = opening_text.starts_with("%w");
            let beginning = if qwords {
                self.token(opening.unwrap(), |value| Kind::QWordsBeg { value })
            } else {
                self.token(opening.unwrap(), |value| Kind::QSymbolsBeg { value })
            };
            let mut loc = self.loc(beginning);
            let mut elements = Vec::new();
            for element in n.elements().iter() {
                let s = if qwords {
                    match element.as_string_node() {
                        Some(string) => span(&string.content_loc()),
                        None => node_span(&element),
                    }
                } else {
                    match element.as_symbol_node() {
                        Some(symbol) => span(&symbol.content_loc()),
                        None => node_span(&element),
                    }
                };
                let content = self.tstring_content(s);
                loc = loc.to(&self.loc(content));
                elements.push(content);
            }
            let tstring_end = self.token_loc(closing.unwrap());
            let loc = loc.to(&tstring_end);
            return if qwords {
                self.add(
                    Kind::QWords {
                        beginning,
                        elements,
                    },
                    loc,
                )
            } else {
                self.add(
                    Kind::QSymbols {
                        beginning,
                        elements,
                    },
                    loc,
                )
            };
        }

        if opening_text.starts_with("%W") || opening_text.starts_with("%I") {
            let words = opening_text.starts_with("%W");
            let beginning = if words {
                self.token(opening.unwrap(), |value| Kind::WordsBeg { value })
            } else {
                self.token(opening.unwrap(), |value| Kind::SymbolsBeg { value })
            };
            let mut loc = self.loc(beginning);
            let mut elements = Vec::new();
            for element in n.elements().iter() {
                let parts: Vec<NodeId> = if let Some(string) = element.as_string_node() {
                    vec![self.tstring_content(span(&string.content_loc()))]
                } else if let Some(symbol) = element.as_symbol_node() {
                    vec![self.tstring_content(span(&symbol.content_loc()))]
                } else if let Some(interp) = element.as_interpolated_string_node() {
                    interp
                        .parts()
                        .iter()
                        .map(|p| self.string_part(&p))
                        .collect()
                } else if let Some(interp) = element.as_interpolated_symbol_node() {
                    interp
                        .parts()
                        .iter()
                        .map(|p| self.string_part(&p))
                        .collect()
                } else {
                    vec![self.visit(&element)]
                };
                let word_loc = match (parts.first(), parts.last()) {
                    (Some(&f), Some(&l)) => self.to(f, l),
                    _ => self.fixed_loc(node_span(&element).start),
                };
                let word = self.add(Kind::Word { parts }, word_loc);
                loc = loc.to(&word_loc);
                elements.push(word);
            }
            let tstring_end = self.token_loc(closing.unwrap());
            let loc = loc.to(&tstring_end);
            return if words {
                self.add(
                    Kind::Words {
                        beginning,
                        elements,
                    },
                    loc,
                )
            } else {
                self.add(
                    Kind::Symbols {
                        beginning,
                        elements,
                    },
                    loc,
                )
            };
        }

        // A bracketed array (or a bracketless one outside of a write value,
        // which shouldn't happen).
        let lbracket = opening.map(|s| self.token(s, |value| Kind::LBracket { value }));
        let elements: Vec<pm::Node> = n.elements().iter().collect();
        let contents = if elements.is_empty() {
            None
        } else {
            let fallback = node_span(&elements[0]).start;
            Some(self.arguments_list(&elements, fallback))
        };
        let loc = match (lbracket, closing) {
            (Some(l), Some(c)) => {
                let rbracket = self.token_loc(c);
                self.loc(l).to(&rbracket)
            }
            _ => self.token_loc(node_span(node)),
        };
        self.add(Kind::ArrayLiteral { lbracket, contents }, loc)
    }

    /// One part of a string-like node: content, `#{}` or `#@var`.
    fn string_part(&mut self, part: &pm::Node<'_>) -> NodeId {
        match part {
            pm::Node::StringNode { .. } => {
                let s = part.as_string_node().unwrap();
                self.tstring_content(span(&s.content_loc()))
            }
            pm::Node::EmbeddedStatementsNode { .. } => {
                let e = part.as_embedded_statements_node().unwrap();
                let beg = self.token_loc(span(&e.opening_loc()));
                let end = self.token_loc(span(&e.closing_loc()));
                let statements = match e.statements() {
                    None => {
                        let void = self.void_stmt(beg.end_char);
                        let loc = self.loc(void);
                        self.add(Kind::Statements { body: vec![void] }, loc)
                    }
                    Some(stmts) => {
                        let body: Vec<NodeId> =
                            stmts.body().iter().map(|s| self.visit(&s)).collect();
                        self.statements(body, beg.end_char)
                    }
                };
                self.bind(statements, beg.end_char, end.start_char);
                let stmts_end_line = self.loc(statements).end_line;
                let loc = Location {
                    start_line: beg.start_line,
                    start_char: beg.start_char,
                    start_column: beg.start_column,
                    end_line: end.end_line.max(stmts_end_line),
                    end_char: end.end_char,
                    end_column: end.end_column,
                };
                self.add(Kind::StringEmbExpr { statements }, loc)
            }
            pm::Node::EmbeddedVariableNode { .. } => {
                let e = part.as_embedded_variable_node().unwrap();
                let embvar = self.token_loc(span(&e.operator_loc()));
                let variable = self.visit(&e.variable());
                let loc = embvar.to(&self.loc(variable));
                self.add(Kind::StringDVar { variable }, loc)
            }
            pm::Node::InterpolatedStringNode { .. } => {
                // Nested parts (from adjacent heredoc lines); flatten into a
                // content token over the whole source range.
                self.tstring_content(node_span(part))
            }
            _ => self.visit(part),
        }
    }

    fn string(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_string_node().unwrap();
        let opening = n.opening_loc().map(|l| span(&l));
        let opening_text = opening.map(|s| self.slice(s)).unwrap_or("");
        if opening_text.starts_with("<<") {
            let items = vec![HeredocItem::Content(span(&n.content_loc()))];
            let parts = self.heredoc_parts(opening.unwrap(), items);
            return self.heredoc(opening.unwrap(), parts, span(&n.closing_loc().unwrap()));
        }
        if opening_text == "?" {
            return self.token(node_span(node), |value| Kind::CHAR { value });
        }
        let content = span(&n.content_loc());
        let parts = if content.start == content.end {
            vec![]
        } else {
            vec![self.tstring_content(content)]
        };
        match (opening, n.closing_loc()) {
            (Some(open), Some(close)) => self.string_literal(open, parts, span(&close)),
            _ => {
                // A string without delimiters (e.g. inside %w handled
                // elsewhere); treat it as content.
                match parts.first() {
                    Some(&p) => p,
                    None => self.tstring_content(content),
                }
            }
        }
    }

    /// Rebuilds a heredoc's parts the way Ripper's lexer emits them: content
    /// runs up to each interpolation, split per line for `<<~` heredocs.
    /// Prism drops whitespace-only content before an interpolation in `<<~`
    /// heredocs, so same-line gaps are restored from the source.
    fn heredoc_parts(&mut self, opening: Span, items: Vec<HeredocItem<'_>>) -> Vec<NodeId> {
        let squiggly = self.slice(opening).starts_with("<<~");
        let mut filled: Vec<HeredocItem> = Vec::new();
        let mut prev_end: Option<usize> = None;
        for item in items {
            let (start, end) = item.bounds();
            let gap_from = prev_end.unwrap_or_else(|| self.line_starts[self.line_of(start) - 1]);
            if start > gap_from {
                let gap = &self.bytes[gap_from..start];
                if gap.iter().all(|&b| b == b' ' || b == b'\t') {
                    filled.push(HeredocItem::Content(Span {
                        start: gap_from,
                        end: start,
                    }));
                }
            }
            if !matches!(item, HeredocItem::Content(s) if s.start == s.end) {
                filled.push(item);
            }
            prev_end = Some(end);
        }

        let mut parts = Vec::new();
        let mut run: Option<Span> = None;
        let flush = |this: &mut Self, run: &mut Option<Span>, parts: &mut Vec<NodeId>| {
            if let Some(r) = run.take() {
                if squiggly {
                    let mut start = r.start;
                    for line in this.source[r.start..r.end].split_inclusive('\n') {
                        let end = start + line.len();
                        parts.push(this.tstring_content(Span { start, end }));
                        start = end;
                    }
                } else {
                    parts.push(this.tstring_content(r));
                }
            }
        };
        for item in filled {
            match item {
                HeredocItem::Content(s) => {
                    run = Some(match run {
                        Some(r) if r.end == s.start => Span {
                            start: r.start,
                            end: s.end,
                        },
                        Some(_) => {
                            flush(self, &mut run, &mut parts);
                            s
                        }
                        None => s,
                    });
                }
                HeredocItem::Embed(node) => {
                    flush(self, &mut run, &mut parts);
                    parts.push(self.string_part(&node));
                }
            }
        }
        flush(self, &mut run, &mut parts);
        parts
    }

    /// `on_string_literal` for a non-heredoc string.
    fn string_literal(&mut self, opening: Span, parts: Vec<NodeId>, closing: Span) -> NodeId {
        let tstring_beg = self.token_loc(opening);
        let tstring_end = self.token_loc(closing);
        let content_end_line = match (parts.first(), parts.last()) {
            (Some(&f), Some(&l)) => self.to(f, l).end_line,
            _ => self.line_of(opening.end),
        };
        let loc = Location {
            start_line: tstring_beg.start_line,
            start_char: tstring_beg.start_char,
            start_column: tstring_beg.start_column,
            end_line: tstring_end.end_line.max(content_end_line),
            end_char: tstring_end.end_char,
            end_column: tstring_end.end_column,
        };
        let quote = self.text(opening);
        self.add(
            Kind::StringLiteral {
                parts,
                quote: Some(quote),
            },
            loc,
        )
    }

    fn heredoc(&mut self, opening: Span, parts: Vec<NodeId>, closing: Span) -> NodeId {
        let beginning = self.token(opening, |value| Kind::HeredocBeg { value });
        let closing_text = self.slice(closing);
        let value = closing_text.strip_suffix('\n').unwrap_or(closing_text);
        let value = value.strip_suffix('\r').unwrap_or(value).to_string();
        let ending_loc = self.token_loc(closing);
        let ending = self.add(Kind::HeredocEnd { value }, ending_loc);
        let begin_loc = self.loc(beginning);
        let loc = Location {
            start_line: begin_loc.start_line,
            start_char: begin_loc.start_char,
            start_column: begin_loc.start_column,
            end_line: ending_loc.end_line,
            end_char: ending_loc.end_char,
            end_column: ending_loc.end_column,
        };
        let dedent = if self.slice(opening).starts_with("<<~") {
            self.heredoc_dedent(&parts)
        } else {
            0
        };
        self.add(
            Kind::Heredoc {
                beginning,
                ending: Some(ending),
                dedent,
                parts,
            },
            loc,
        )
    }

    /// `visit_heredoc_node_whitespace`: the common leading whitespace.
    fn heredoc_dedent(&self, parts: &[NodeId]) -> usize {
        let mut common: Option<usize> = None;
        let mut dedent_next = true;
        for &part in parts {
            if let Kind::TStringContent { value } = self.kind(part) {
                let trimmed = value.strip_suffix('\n').unwrap_or(value);
                if dedent_next && !trimmed.is_empty() {
                    let mut width = 0;
                    for c in value.chars() {
                        if c == '\t' {
                            width = (width / 8 + 1) * 8;
                        } else if c.is_whitespace() {
                            width += 1;
                        } else {
                            break;
                        }
                    }
                    common = Some(common.map_or(width, |c| c.min(width)));
                }
                dedent_next = value.ends_with('\n');
            } else {
                dedent_next = false;
            }
        }
        // Ripper reports INT_MAX when there is no line to measure.
        common.unwrap_or(i32::MAX as usize)
    }

    fn interpolated_string(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_interpolated_string_node().unwrap();
        let opening = n.opening_loc().map(|l| span(&l));
        let opening_text = opening.map(|s| self.slice(s)).unwrap_or("");
        if opening_text.starts_with("<<") {
            let mut items = Vec::new();
            for part in n.parts().iter() {
                collect_heredoc_items(part, &mut items);
            }
            let parts = self.heredoc_parts(opening.unwrap(), items);
            return self.heredoc(opening.unwrap(), parts, span(&n.closing_loc().unwrap()));
        }
        let pparts: Vec<pm::Node> = n.parts().iter().collect();
        let is_concat = opening.is_none()
            && pparts.len() > 1
            && pparts.iter().any(|p| {
                p.as_string_node()
                    .is_some_and(|s| s.opening_loc().is_some())
                    || p.as_interpolated_string_node()
                        .is_some_and(|s| s.opening_loc().is_some())
            });
        if is_concat {
            let mut iter = pparts.iter();
            let first = iter.next().unwrap();
            let mut left = self.visit(first);
            for part in iter {
                let right = self.visit(part);
                let loc = self.to(left, right);
                left = self.add(Kind::StringConcat { left, right }, loc);
            }
            return left;
        }
        let parts: Vec<NodeId> = pparts.iter().map(|p| self.string_part(p)).collect();
        match (opening, n.closing_loc()) {
            (Some(open), Some(close)) => self.string_literal(open, parts, span(&close)),
            _ => {
                let loc = self.token_loc(node_span(node));
                self.add(Kind::StringLiteral { parts, quote: None }, loc)
            }
        }
    }

    fn xstring(&mut self, node: &pm::Node<'_>) -> NodeId {
        let (opening, parts, closing) = if let Some(n) = node.as_x_string_node() {
            let opening = span(&n.opening_loc());
            let content = span(&n.content_loc());
            let parts = if self.slice(opening).starts_with("<<") {
                self.heredoc_parts(opening, vec![HeredocItem::Content(content)])
            } else if content.start == content.end {
                vec![]
            } else {
                vec![self.tstring_content(content)]
            };
            (opening, parts, span(&n.closing_loc()))
        } else {
            let n = node.as_interpolated_x_string_node().unwrap();
            let opening = span(&n.opening_loc());
            let heredoc = self.slice(opening).starts_with("<<");
            let parts = if heredoc {
                let mut items = Vec::new();
                for part in n.parts().iter() {
                    collect_heredoc_items(part, &mut items);
                }
                self.heredoc_parts(opening, items)
            } else {
                n.parts().iter().map(|p| self.string_part(&p)).collect()
            };
            (opening, parts, span(&n.closing_loc()))
        };
        if self.slice(opening).starts_with("<<") {
            return self.heredoc(opening, parts, closing);
        }
        let backtick = self.token_loc(opening);
        let mut loc = backtick;
        for &part in &parts {
            loc = loc.to(&self.loc(part));
        }
        let ending = self.token_loc(closing);
        let loc = loc.to(&ending);
        self.add(Kind::XStringLiteral { parts }, loc)
    }

    fn regexp(&mut self, opening: Span, parts: Vec<NodeId>, closing: Span) -> NodeId {
        let beginning_loc = self.token_loc(opening);
        let mut loc = beginning_loc;
        for &part in &parts {
            loc = loc.to(&self.loc(part));
        }
        let ending_loc = self.token_loc(closing);
        let loc = loc.to(&ending_loc);
        let beginning = self.text(opening);
        let ending = self.text(closing);
        self.add(
            Kind::RegexpLiteral {
                beginning,
                ending,
                parts,
            },
            loc,
        )
    }

    fn symbol(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_symbol_node().unwrap();
        let opening = n.opening_loc().map(|l| span(&l));
        let closing = n.closing_loc().map(|l| span(&l));
        let opening_text = opening.map(|s| self.slice(s)).unwrap_or("");
        let closing_text = closing.map(|s| self.slice(s)).unwrap_or("");
        let dyna = opening_text.starts_with("%s")
            || opening_text.ends_with('\'')
            || opening_text.ends_with('"')
            || opening_text.ends_with("':")
            || opening_text.ends_with("\":");
        if dyna {
            let value = Some(span(&n.content_loc()));
            let parts = match value {
                Some(v) if v.start != v.end => vec![self.tstring_content(v)],
                _ => vec![],
            };
            return self.dyna_symbol(opening, parts, closing, node_span(node));
        }
        if closing_text == ":" {
            return self.token(node_span(node), |value| Kind::Label { value });
        }
        let value_span = span(&n.content_loc());
        if opening.is_none() && closing.is_none() {
            let value = self.visit_token(value_span, true);
            let loc = self.loc(value);
            return self.add(Kind::SymbolLiteral { value }, loc);
        }
        let value = self.visit_token(value_span, true);
        let symbeg = self.token_loc(opening.unwrap());
        let loc = symbeg.to(&self.loc(value));
        self.add(Kind::SymbolLiteral { value }, loc)
    }

    fn dyna_symbol(
        &mut self,
        opening: Option<Span>,
        parts: Vec<NodeId>,
        closing: Option<Span>,
        whole: Span,
    ) -> NodeId {
        let opening_text = opening.map(|s| self.slice(s)).unwrap_or("");
        if opening_text.starts_with(':') || opening_text.starts_with("%s") {
            let symbeg = self.token_loc(opening.unwrap());
            let ending = self.token_loc(closing.unwrap_or(whole));
            let loc = symbeg.to(&ending);
            let quote = self.text(opening.unwrap());
            self.add(
                Kind::DynaSymbol {
                    parts,
                    quote: Some(quote),
                },
                loc,
            )
        } else {
            // A dynamic symbol as a hash key: "foo": value
            let tstring_beg = self.token_loc(opening.unwrap_or(whole));
            let label_end_span = closing.unwrap_or(whole);
            let label_end = self.token_loc(label_end_span);
            let loc = tstring_beg.to(&label_end);
            let quote = self
                .slice(label_end_span)
                .chars()
                .next()
                .map(|c| c.to_string());
            self.add(Kind::DynaSymbol { parts, quote }, loc)
        }
    }

    // ------------------------------------------------------------------
    // Assignment targets
    // ------------------------------------------------------------------

    fn multi_write(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_multi_write_node().unwrap();
        let lefts: Vec<pm::Node> = n.lefts().iter().collect();
        let rights: Vec<pm::Node> = n.rights().iter().collect();
        let mut target = self.mlhs_targets(&lefts, n.rest(), &rights, true);
        if let (Some(l), Some(r)) = (n.lparen_loc(), n.rparen_loc()) {
            target = self.mlhs_paren(target, span(&l), span(&r));
        }
        let value = self.visit_write_value(&n.value());
        // on_massign: a trailing comma before the `=` destructures.
        let between = Span {
            start: self.loc(target).end_char,
            end: self.loc(value).start_char,
        };
        if between.start <= between.end && self.slice(between).trim().starts_with(',') {
            self.set_comma(target);
        }
        let loc = self.to(target, value);
        self.add(Kind::MAssign { target, value }, loc)
    }

    fn set_comma(&mut self, id: NodeId) {
        match &mut self.nodes[id as usize].kind {
            Kind::MLHS { comma, .. } | Kind::MLHSParen { comma, .. } => *comma = true,
            _ => {}
        }
    }

    /// `visit_multi_target_node_targets`
    fn mlhs_targets(
        &mut self,
        lefts: &[pm::Node<'_>],
        rest: Option<pm::Node<'_>>,
        rights: &[pm::Node<'_>],
        skippable: bool,
    ) -> NodeId {
        if skippable
            && lefts.len() == 1
            && lefts[0].as_multi_target_node().is_some()
            && rest.is_none()
            && rights.is_empty()
        {
            return self.visit(&lefts[0]);
        }
        let mut parts: Vec<NodeId> = Vec::new();
        let mut location: Option<Location> = None;
        for left in lefts {
            let part = self.visit(left);
            let part_loc = self.loc(part);
            location = Some(match location {
                Some(l) if !parts.is_empty() => l.to(&part_loc),
                _ => part_loc,
            });
            parts.push(part);
        }
        if let Some(rest) = rest
            && let Some(splat) = rest.as_splat_node()
        {
            let star = self.arg_star(&splat);
            let star_loc = self.loc(star);
            location = Some(match location {
                Some(l) if !parts.is_empty() => l.to(&star_loc),
                _ => star_loc,
            });
            parts.push(star);
        }
        if !rights.is_empty() {
            let mut post_loc: Option<Location> = None;
            let mut post_parts = Vec::new();
            for right in rights {
                let part = self.visit(right);
                let part_loc = self.loc(part);
                post_loc = Some(match post_loc {
                    Some(l) => l.to(&part_loc),
                    None => part_loc,
                });
                post_parts.push(part);
            }
            let post_loc = post_loc.unwrap();
            location = Some(match location {
                Some(l) => l.to(&post_loc),
                None => post_loc,
            });
            parts.extend(post_parts);
        }
        let loc = location.unwrap_or_else(|| {
            let start = lefts.first().map(|l| node_span(l).start).unwrap_or(0);
            self.fixed_loc(start)
        });
        self.add(
            Kind::MLHS {
                parts,
                comma: false,
            },
            loc,
        )
    }

    fn mlhs_paren(&mut self, contents: NodeId, lparen: Span, rparen: Span) -> NodeId {
        let lparen_loc = self.token_loc(lparen);
        let rparen_loc = self.token_loc(rparen);
        let inner = Span {
            start: lparen.end,
            end: rparen.start,
        };
        if self.slice(inner).trim().ends_with(',') {
            self.set_comma(contents);
        }
        let loc = lparen_loc.to(&rparen_loc);
        self.add(
            Kind::MLHSParen {
                contents,
                comma: false,
            },
            loc,
        )
    }

    // ------------------------------------------------------------------
    // Definitions
    // ------------------------------------------------------------------

    fn def(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_def_node().unwrap();
        let target = n.receiver().map(|r| self.visit(&r));
        let operator = match n.operator_loc() {
            Some(op) => CallOperator::Node(self.visit_token(span(&op), true)),
            None => CallOperator::Nil,
        };
        let name = self.visit_token(span(&n.name_loc()), true);
        let name_loc = self.loc(name);

        let mut params = match n.parameters() {
            Some(p) => self.parameters(&p),
            None => self.empty_params(name_loc.end_char),
        };
        if let Some(lparen) = n.lparen_loc() {
            params = self.paren_params(params, span(&lparen), span(&n.rparen_loc().unwrap()));
        } else if self.params_empty(params) {
            // on_def: an empty Params is located right after the name.
            let mut loc = self.loc(params);
            loc.start_char = name_loc.end_char;
            loc.start_column = name_loc.end_column;
            loc.end_char = name_loc.end_char;
            loc.end_column = name_loc.end_column;
            self.set_loc(params, loc);
        }

        let keyword = self.kw(span(&n.def_keyword_loc()));

        if n.equal_loc().is_some() {
            // Endless method: the body is a Statements with the expression.
            let body = n.body().unwrap();
            let expression = match body.as_statements_node() {
                Some(stmts) => {
                    let first = stmts.body().iter().next().unwrap();
                    self.visit(&first)
                }
                None => self.visit(&body),
            };
            let loc = self.loc(expression);
            let statements = self.add(
                Kind::Statements {
                    body: vec![expression],
                },
                loc,
            );
            let def_loc = self.loc(keyword).to(&loc);
            return self.add(
                Kind::DefNode {
                    target,
                    operator,
                    name,
                    params: Some(params),
                    bodystmt: statements,
                },
                def_loc,
            );
        }

        let end_keyword = span(&n.end_keyword_loc().unwrap());
        let void_from = n.rparen_loc().map(|r| span(&r)).unwrap_or(end_keyword);
        let bodystmt = self.body_node(n.body(), void_from, false, end_keyword.start);
        let ending = self.token_loc(end_keyword);
        let start_char = self.find_next_statement_start(self.loc(params).end_char);
        self.bind_bodystmt(bodystmt, start_char, ending.start_char);
        let loc = self.loc(keyword).to(&ending);
        self.add(
            Kind::DefNode {
                target,
                operator,
                name,
                params: Some(params),
                bodystmt,
            },
            loc,
        )
    }

    fn params_empty(&self, params: NodeId) -> bool {
        match self.kind(params) {
            Kind::Params {
                requireds,
                optionals,
                rest,
                posts,
                keywords,
                keyword_rest,
                block,
            } => {
                requireds.is_empty()
                    && optionals.is_empty()
                    && rest.is_none()
                    && posts.is_empty()
                    && keywords.is_empty()
                    && *keyword_rest == NilOr::None
                    && block.is_none()
            }
            _ => false,
        }
    }

    fn empty_params(&mut self, offset: usize) -> NodeId {
        let loc = self.fixed_loc(offset);
        self.add(
            Kind::Params {
                requireds: vec![],
                optionals: vec![],
                rest: None,
                posts: vec![],
                keywords: vec![],
                keyword_rest: NilOr::None,
                block: None,
            },
            loc,
        )
    }

    /// `on_paren` for a parameter list: the Params location is rebound to the
    /// inside of the parentheses.
    fn paren_params(&mut self, params: NodeId, lparen: Span, rparen: Span) -> NodeId {
        let lparen_node = self.token(lparen, |value| Kind::LParen { value });
        let rparen_loc = self.token_loc(rparen);
        let start_char = self.find_next_statement_start(lparen.end);
        let mut loc = self.loc(params);
        loc.start_char = start_char;
        loc.start_column = self.column_of(start_char);
        loc.end_char = rparen_loc.start_char;
        loc.end_column = rparen_loc.start_column;
        self.set_loc(params, loc);
        let loc = self.loc(lparen_node).to(&rparen_loc);
        self.add(
            Kind::Paren {
                lparen: lparen_node,
                contents: Some(params),
            },
            loc,
        )
    }

    fn parameters(&mut self, params: &pm::ParametersNode<'_>) -> NodeId {
        let mut locs: Vec<Location> = Vec::new();
        let requireds: Vec<NodeId> = params
            .requireds()
            .iter()
            .map(|r| self.required_param(&r))
            .collect();
        for &r in &requireds {
            locs.push(self.loc(r));
        }
        let mut optionals = Vec::new();
        for optional in params.optionals().iter() {
            let o = optional.as_optional_parameter_node().unwrap();
            let name = self.visit_token(span(&o.name_loc()), true);
            let value = self.visit(&o.value());
            locs.push(self.loc(name));
            locs.push(self.loc(value));
            optionals.push((name, value));
        }
        let rest = params.rest().map(|r| self.rest_param(&r));
        if let Some(r) = rest {
            locs.push(self.loc(r));
        }
        let posts: Vec<NodeId> = params
            .posts()
            .iter()
            .map(|p| self.required_param(&p))
            .collect();
        for &p in &posts {
            locs.push(self.loc(p));
        }
        let mut keywords = Vec::new();
        for keyword in params.keywords().iter() {
            if let Some(k) = keyword.as_required_keyword_parameter_node() {
                let label = self.token(span(&k.name_loc()), |value| Kind::Label { value });
                locs.push(self.loc(label));
                keywords.push((label, None));
            } else if let Some(k) = keyword.as_optional_keyword_parameter_node() {
                let label = self.token(span(&k.name_loc()), |value| Kind::Label { value });
                let value = self.visit(&k.value());
                locs.push(self.loc(label));
                locs.push(self.loc(value));
                keywords.push((label, Some(value)));
            }
        }
        let keyword_rest = match params.keyword_rest() {
            None => NilOr::None,
            Some(kr) => {
                if kr.as_no_keywords_parameter_node().is_some()
                    || kr.as_keyword_rest_parameter_node().is_some()
                {
                    self.consume_pow_before(node_span(&kr).end);
                }
                if let Some(nokw) = kr.as_no_keywords_parameter_node() {
                    let op = self.token_loc(span(&nokw.operator_loc()));
                    let kw = self.token_loc(span(&nokw.keyword_loc()));
                    locs.push(op.to(&kw));
                    NilOr::NilSymbol
                } else if let Some(k) = kr.as_keyword_rest_parameter_node() {
                    let name = k.name_loc().map(|l| self.ident(span(&l)));
                    let op = self.token_loc(span(&k.operator_loc()));
                    let loc = match name {
                        Some(n) => op.to(&self.loc(n)),
                        None => op,
                    };
                    let id = self.add(Kind::KwRestParam { name }, loc);
                    locs.push(loc);
                    NilOr::Node(id)
                } else {
                    let id = self.visit(&kr);
                    locs.push(self.loc(id));
                    NilOr::Node(id)
                }
            }
        };
        let block = params
            .block()
            .and_then(|b| b.as_block_parameter_node())
            .map(|b| {
                let name = b.name_loc().map(|l| self.ident(span(&l)));
                let op = self.token_loc(span(&b.operator_loc()));
                let loc = match name {
                    Some(n) => op.to(&self.loc(n)),
                    None => op,
                };
                self.add(Kind::BlockArg { name }, loc)
            });
        if let Some(b) = block {
            locs.push(self.loc(b));
        }
        let loc = match (locs.first(), locs.last()) {
            (Some(f), Some(l)) => f.to(l),
            _ => self.fixed_loc(span(&params.location()).start),
        };
        self.add(
            Kind::Params {
                requireds,
                optionals,
                rest,
                posts,
                keywords,
                keyword_rest,
                block,
            },
            loc,
        )
    }

    fn required_param(&mut self, node: &pm::Node<'_>) -> NodeId {
        if let Some(m) = node.as_multi_target_node() {
            let lefts: Vec<pm::Node> = m.lefts().iter().collect();
            let rights: Vec<pm::Node> = m.rights().iter().collect();
            let targets = self.mlhs_targets(&lefts, m.rest(), &rights, false);
            return self.mlhs_paren(
                targets,
                span(&m.lparen_loc().unwrap()),
                span(&m.rparen_loc().unwrap()),
            );
        }
        if node.as_required_parameter_node().is_some() {
            return self.ident(node_span(node));
        }
        self.visit(node)
    }

    fn rest_param(&mut self, node: &pm::Node<'_>) -> NodeId {
        if let Some(r) = node.as_rest_parameter_node() {
            let name = r.name_loc().map(|l| self.visit_token(span(&l), true));
            let op = self.token_loc(span(&r.operator_loc()));
            let loc = match name {
                Some(n) => op.to(&self.loc(n)),
                None => op,
            };
            return self.add(Kind::RestParam { name }, loc);
        }
        if node.as_implicit_rest_node().is_some() {
            let loc = self.token_loc(node_span(node));
            return self.add(
                Kind::ExcessedComma {
                    value: ",".to_string(),
                },
                loc,
            );
        }
        self.visit(node)
    }

    /// `visit_body_node`: a BodyStmt for def/class/module/do-block bodies.
    fn body_node(
        &mut self,
        body: Option<pm::Node<'_>>,
        void_from: Span,
        allow_newline: bool,
        empty_offset: usize,
    ) -> NodeId {
        match body {
            None => {
                let statements = self.statements(vec![], empty_offset);
                self.bodystmt(statements, None, None, None)
            }
            Some(body) => {
                if let Some(stmts) = body.as_statements_node() {
                    let statements = self.statements_with_void(&stmts, void_from, allow_newline);
                    self.bodystmt(statements, None, None, None)
                } else if let Some(begin) = body.as_begin_node() {
                    self.begin_clauses(&begin, void_from, allow_newline)
                } else {
                    let inner = self.visit(&body);
                    let loc = self.loc(inner);
                    let statements = self.add(Kind::Statements { body: vec![inner] }, loc);
                    self.bodystmt(statements, None, None, None)
                }
            }
        }
    }

    /// Visits a StatementsNode, inserting Ripper's leading `void_stmt` when the
    /// body starts after a semicolon (or a newline, for class/module bodies).
    fn statements_with_void(
        &mut self,
        stmts: &pm::StatementsNode<'_>,
        void_from: Span,
        allow_newline: bool,
    ) -> NodeId {
        let first = stmts.body().iter().next();
        let mut body = Vec::new();
        if let Some(first) = &first {
            let first_start = node_span(first).start;
            if void_from.end <= first_start {
                let gap = &self.bytes[void_from.end..first_start];
                if gap.contains(&b';') || (allow_newline && gap.contains(&b'\n')) {
                    body.push(self.void_stmt(void_from.end));
                }
            }
        }
        for stmt in stmts.body().iter() {
            body.push(self.visit(&stmt));
        }
        self.statements(body, void_from.end)
    }

    /// `on_bodystmt`
    fn bodystmt(
        &mut self,
        statements: NodeId,
        rescue_clause: Option<NodeId>,
        else_parts: Option<(NodeId, NodeId)>,
        ensure_clause: Option<NodeId>,
    ) -> NodeId {
        let (else_keyword, else_clause) = match else_parts {
            Some((kw, clause)) => (Some(kw), Some(clause)),
            None => (None, None),
        };
        let parts: Vec<NodeId> = [Some(statements), rescue_clause, else_clause, ensure_clause]
            .into_iter()
            .flatten()
            .collect();
        let loc = self.to(parts[0], *parts.last().unwrap());
        self.add(
            Kind::BodyStmt {
                statements,
                rescue_clause,
                else_keyword,
                else_clause,
                ensure_clause,
            },
            loc,
        )
    }

    /// `visit_begin_node_clauses`
    fn begin_clauses(
        &mut self,
        node: &pm::BeginNode<'_>,
        void_from: Span,
        allow_newline: bool,
    ) -> NodeId {
        let statements = match node.statements() {
            None => {
                let offset = node
                    .rescue_clause()
                    .map(|r| span(&r.location()).start)
                    .or_else(|| node.else_clause().map(|e| span(&e.location()).start))
                    .or_else(|| node.ensure_clause().map(|e| span(&e.location()).start))
                    .unwrap_or(void_from.end);
                self.statements(vec![], offset)
            }
            Some(stmts) => self.statements_with_void(&stmts, void_from, allow_newline),
        };
        let rescue_clause = node.rescue_clause().map(|r| self.rescue(&r));
        let else_parts = node.else_clause().map(|e| {
            let keyword = span(&e.else_keyword_loc());
            let clause = match e.statements() {
                // Never bound, so it keeps Ripper's lexer position: the end of
                // the token after the else keyword.
                None => {
                    let at = self.next_token_end(keyword.end);
                    let statements = self.statements(vec![], at);
                    let loc = self.fixed_loc(at);
                    if let Kind::Statements { body } = self.kind(statements) {
                        let void = body[0];
                        self.set_loc(void, loc);
                    }
                    self.set_loc(statements, loc);
                    statements
                }
                Some(stmts) => self.statements_with_void(&stmts, keyword, false),
            };
            let kw = self.kw(keyword);
            (kw, clause)
        });
        let ensure_clause = node.ensure_clause().map(|e| self.ensure(&e));
        self.bodystmt(statements, rescue_clause, else_parts, ensure_clause)
    }

    fn rescue(&mut self, node: &pm::RescueNode<'_>) -> NodeId {
        // Ripper reduces the chain right to left, so later clauses bind their
        // statements (and grab comments) first.
        let consequent = node.subsequent().map(|s| self.rescue(&s));
        let exceptions: Vec<pm::Node> = node.exceptions().iter().collect();
        let exception_node = match exceptions.len() {
            0 => None,
            1 => {
                if let Some(splat) = exceptions[0].as_splat_node() {
                    let star = self.arg_star(&splat);
                    let loc = self.loc(star);
                    Some(self.add(Kind::MRHS { parts: vec![star] }, loc))
                } else {
                    Some(self.visit(&exceptions[0]))
                }
            }
            _ => {
                let args = self.arguments_list(&exceptions, node_span(&exceptions[0]).start);
                let (parts, loc) = match self.kind(args) {
                    Kind::Args { parts } => (parts.clone(), self.loc(args)),
                    _ => unreachable!(),
                };
                Some(self.add(Kind::MRHS { parts }, loc))
            }
        };
        let variable = node.reference().map(|r| self.visit(&r));
        let keyword = self.kw(span(&node.keyword_loc()));
        let keyword_loc = self.loc(keyword);
        let statements = match node.statements() {
            None => self.statements(vec![], keyword_loc.end_char),
            Some(stmts) => {
                let body: Vec<NodeId> = stmts.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, keyword_loc.end_char)
            }
        };

        // char_pos when on_rescue fires: the start of the token after the
        // whole rescue chain (the next else/ensure/end).
        let reduce_pos = self.rescue_chain_end(node);

        let last_node = variable.or(exception_node).unwrap_or(keyword);
        let start_char = self.find_next_statement_start(self.loc(last_node).end_char);
        let next_token = match node.subsequent() {
            Some(next) => span(&next.keyword_loc()).start,
            None => reduce_pos,
        };
        self.next_void_line = Some(self.line_of(next_token.min(self.bytes.len())));
        self.bind(statements, start_char, reduce_pos);
        self.next_void_line = None;

        let exception = if exception_node.is_some() || variable.is_some() {
            let last = self.loc(last_node);
            let loc = Location {
                start_line: keyword_loc.start_line,
                start_char: keyword_loc.end_char + 1,
                start_column: keyword_loc.end_column + 1,
                end_line: last.end_line,
                end_char: last.end_char,
                end_column: last.end_column,
            };
            Some(self.add(
                Kind::RescueEx {
                    exceptions: exception_node,
                    variable,
                },
                loc,
            ))
        } else {
            None
        };
        let loc = Location {
            start_line: keyword_loc.start_line,
            start_char: keyword_loc.start_char,
            start_column: keyword_loc.start_column,
            end_line: self.line_of(reduce_pos),
            end_char: reduce_pos,
            end_column: self.column_of(reduce_pos.min(self.bytes.len())),
        };
        self.add(
            Kind::Rescue {
                keyword,
                exception,
                statements,
                consequent,
            },
            loc,
        )
    }

    /// The offset of the token that follows a rescue chain.
    fn rescue_chain_end(&self, node: &pm::RescueNode<'_>) -> usize {
        let mut last = node.location();
        let mut current = node.subsequent();
        while let Some(next) = current {
            last = next.location();
            current = next.subsequent();
        }
        let mut position = last.end() as usize;
        while position < self.bytes.len() {
            match self.bytes[position] {
                b' ' | b'\t' | b'\n' | b'\r' | b';' => position += 1,
                b'#' => {
                    while position < self.bytes.len() && self.bytes[position] != b'\n' {
                        position += 1;
                    }
                }
                _ => break,
            }
        }
        position
    }

    fn ensure(&mut self, node: &pm::EnsureNode<'_>) -> NodeId {
        let keyword_span = span(&node.ensure_keyword_loc());
        let statements = match node.statements() {
            None => self.statements(vec![], keyword_span.end),
            Some(stmts) => self.statements_with_void(&stmts, keyword_span, false),
        };
        let keyword = self.kw(keyword_span);
        let ending = self.token_loc(span(&node.end_keyword_loc()));
        let start_char = self.find_next_statement_start(keyword_span.end);
        self.bind(statements, start_char, ending.start_char);
        let loc = self.loc(keyword).to(&ending);
        self.add(
            Kind::Ensure {
                keyword,
                statements,
            },
            loc,
        )
    }

    fn begin(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_begin_node().unwrap();
        let Some(begin_keyword) = n.begin_keyword_loc() else {
            // An implicit begin (rescue clauses in a block without `begin`).
            let start = node_span(node).start;
            let s = Span { start, end: start };
            return self.begin_clauses(&n, s, false);
        };
        let begin_span = span(&begin_keyword);
        let bodystmt = self.begin_clauses(&n, begin_span, false);
        let keyword = self.kw(begin_span);
        let has_else = matches!(
            self.kind(bodystmt),
            Kind::BodyStmt {
                else_clause: Some(_),
                ..
            }
        );
        let end_location = if has_else {
            self.loc(bodystmt)
        } else {
            self.token_loc(span(&n.end_keyword_loc().unwrap()))
        };
        let start_char = self.find_next_statement_start(begin_span.end);
        self.bind_bodystmt(bodystmt, start_char, end_location.end_char);
        let loc = self.loc(keyword).to(&end_location);
        self.add(Kind::Begin { bodystmt }, loc)
    }

    fn class(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_class_node().unwrap();
        let constant = self.class_constant(&n.constant_path());
        let superclass = n.superclass().map(|s| self.visit(&s));
        let void_from = match n.superclass() {
            Some(s) => node_span(&s),
            None => node_span(&n.constant_path()),
        };
        let end_keyword = span(&n.end_keyword_loc());
        let bodystmt = self.body_node(n.body(), void_from, superclass.is_none(), end_keyword.start);
        let beginning = self.kw(span(&n.class_keyword_loc()));
        let ending = self.token_loc(end_keyword);
        let location = self.loc(superclass.unwrap_or(constant));
        let start_char = self.find_next_statement_start(location.end_char);
        if superclass.is_none() {
            self.next_void_line = Some(location.end_line);
        }
        self.bind_bodystmt(bodystmt, start_char, ending.start_char);
        self.next_void_line = None;
        let loc = self.loc(beginning).to(&ending);
        self.add(
            Kind::ClassDeclaration {
                constant,
                superclass,
                bodystmt,
            },
            loc,
        )
    }

    fn class_constant(&mut self, node: &pm::Node<'_>) -> NodeId {
        if node.as_constant_read_node().is_some() {
            let constant = self.token(node_span(node), |value| Kind::Const { value });
            let loc = self.loc(constant);
            self.add(Kind::ConstRef { constant }, loc)
        } else {
            self.visit(node)
        }
    }

    fn module(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_module_node().unwrap();
        let constant = self.class_constant(&n.constant_path());
        let end_keyword = span(&n.end_keyword_loc());
        let bodystmt = self.body_node(
            n.body(),
            node_span(&n.constant_path()),
            true,
            end_keyword.start,
        );
        let beginning = self.kw(span(&n.module_keyword_loc()));
        let ending = self.token_loc(end_keyword);
        let start_char = self.find_next_statement_start(self.loc(constant).end_char);
        self.next_void_line = Some(self.loc(constant).end_line);
        self.bind_bodystmt(bodystmt, start_char, ending.start_char);
        self.next_void_line = None;
        let loc = self.loc(beginning).to(&ending);
        self.add(Kind::ModuleDeclaration { constant, bodystmt }, loc)
    }

    fn sclass(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_singleton_class_node().unwrap();
        let target = self.visit(&n.expression());
        let end_keyword = span(&n.end_keyword_loc());
        let void_from = n.body().map(|b| node_span(&b)).unwrap_or(end_keyword);
        let bodystmt = self.body_node(n.body(), void_from, false, end_keyword.start);
        let beginning = self.kw(span(&n.class_keyword_loc()));
        let ending = self.token_loc(end_keyword);
        let start_char = self.find_next_statement_start(self.loc(target).end_char);
        self.bind_bodystmt(bodystmt, start_char, ending.start_char);
        let loc = self.loc(beginning).to(&ending);
        self.add(Kind::SClass { target, bodystmt }, loc)
    }

    fn pre_post_execution(
        &mut self,
        statements: Option<pm::StatementsNode<'_>>,
        keyword: Span,
        opening: Span,
        closing: Span,
    ) -> (NodeId, NodeId, Location) {
        let statements = match statements {
            None => self.statements(vec![], opening.end),
            Some(stmts) => {
                let body: Vec<NodeId> = stmts.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, opening.end)
            }
        };
        let lbrace = self.token(opening, |value| Kind::LBrace { value });
        let rbrace = self.token_loc(closing);
        let start_char = self.find_next_statement_start(opening.end);
        self.bind(statements, start_char, rbrace.start_char);
        let keyword = self.token_loc(keyword);
        (lbrace, statements, keyword.to(&rbrace))
    }

    // ------------------------------------------------------------------
    // Blocks and lambdas
    // ------------------------------------------------------------------

    fn block(&mut self, node: &pm::BlockNode<'_>) -> NodeId {
        let opening_span = span(&node.opening_loc());
        let closing_span = span(&node.closing_loc());
        let braces = self.slice(opening_span) == "{";
        let block_var = node
            .parameters()
            .and_then(|p| p.as_block_parameters_node())
            .map(|p| self.block_var(&p));
        let void_from = node
            .parameters()
            .filter(|p| p.as_block_parameters_node().is_some())
            .map(|p| node_span(&p))
            .unwrap_or(opening_span);

        let body = if braces {
            match node.body() {
                None => self.statements(vec![], opening_span.end),
                Some(body) => match body.as_statements_node() {
                    Some(stmts) => self.statements_with_void(&stmts, void_from, false),
                    None => {
                        let bodystmt =
                            self.body_node(Some(body), void_from, false, opening_span.end);
                        match self.kind(bodystmt) {
                            Kind::BodyStmt { statements, .. } => *statements,
                            _ => bodystmt,
                        }
                    }
                },
            }
        } else {
            self.body_node(node.body(), void_from, false, opening_span.end)
        };

        if braces {
            let lbrace = self.token(opening_span, |value| Kind::LBrace { value });
            let rbrace = self.token_loc(closing_span);
            let location = self.loc(block_var.unwrap_or(lbrace));
            let start_char = self.find_next_statement_start(location.end_char);
            self.bind(body, start_char, rbrace.start_char);
            let lbrace_loc = self.loc(lbrace);
            let loc = Location {
                start_line: lbrace_loc.start_line,
                start_char: lbrace_loc.start_char,
                start_column: lbrace_loc.start_column,
                end_line: rbrace.end_line.max(self.loc(body).end_line),
                end_char: rbrace.end_char,
                end_column: rbrace.end_column,
            };
            self.add(
                Kind::BlockNode {
                    opening: lbrace,
                    block_var,
                    bodystmt: body,
                },
                loc,
            )
        } else {
            let beginning = self.kw(opening_span);
            let ending = self.token_loc(closing_span);
            let location = self.loc(block_var.unwrap_or(beginning));
            let start_char = self.find_next_statement_start(location.end_char);
            self.bind_bodystmt(body, start_char, ending.start_char);
            let loc = self.loc(beginning).to(&ending);
            self.add(
                Kind::BlockNode {
                    opening: beginning,
                    block_var,
                    bodystmt: body,
                },
                loc,
            )
        }
    }

    /// `on_block_var`
    fn block_var(&mut self, node: &pm::BlockParametersNode<'_>) -> NodeId {
        let params = match node.parameters() {
            Some(p) => self.parameters(&p),
            None => self.empty_params(span(&node.location()).start),
        };
        let locals: Vec<NodeId> = node
            .locals()
            .iter()
            .map(|l| self.ident(node_span(&l)))
            .collect();
        let opening = node.opening_loc().map(|l| span(&l));
        let closing = node.closing_loc().map(|l| span(&l));
        let (beginning, ending) = match (opening, closing) {
            (Some(o), Some(c)) => (self.token_loc(o), self.token_loc(c)),
            (Some(o), None) => (self.token_loc(o), self.token_loc(o)),
            _ => {
                let whole = self.token_loc(node_span(&node.as_node()));
                (whole, whole)
            }
        };
        let double_pipe = opening.is_some_and(|o| self.slice(o) == "||");
        if self.params_empty(params) {
            let start_line = self.loc(params).start_line;
            let start_char = if double_pipe {
                beginning.start_char
            } else {
                self.find_next_statement_start(beginning.end_char)
            };
            let column = start_char
                - self.line_starts[start_line.min(self.line_starts.len()) - 1].min(start_char);
            let loc = Location::fixed(start_line, start_char, column);
            self.set_loc(params, loc);
        }
        let loc = beginning.to(&ending);
        self.add(Kind::BlockVar { params, locals }, loc)
    }

    fn lambda(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_lambda_node().unwrap();
        let opening_span = span(&n.opening_loc());
        let closing_span = span(&n.closing_loc());
        let braces = self.slice(opening_span) == "{";

        let block_params = n.parameters().and_then(|p| p.as_block_parameters_node());
        let params = match &block_params {
            Some(bp) => {
                let params = match bp.parameters() {
                    Some(p) => self.parameters(&p),
                    None => self.empty_params(span(&bp.location()).start),
                };
                match (bp.opening_loc(), bp.closing_loc()) {
                    (Some(lparen), Some(rparen)) => {
                        let lparen = span(&lparen);
                        let rparen = span(&rparen);
                        let paren = self.paren_params(params, lparen, rparen);
                        // Re-wrap the Params in a LambdaVar with any locals.
                        let locals: Vec<NodeId> = bp
                            .locals()
                            .iter()
                            .map(|l| self.ident(node_span(&l)))
                            .collect();
                        let mut location = self.loc(params);
                        if let Some(&last) = locals.last() {
                            location = location.to(&self.loc(last));
                        }
                        let lambda_var = self.add(Kind::LambdaVar { params, locals }, location);
                        if let Kind::Paren { contents, .. } = &mut self.nodes[paren as usize].kind {
                            *contents = Some(lambda_var);
                        }
                        paren
                    }
                    _ => {
                        let loc = self.loc(params);
                        self.add(
                            Kind::LambdaVar {
                                params,
                                locals: vec![],
                            },
                            loc,
                        )
                    }
                }
            }
            None => {
                let params = self.empty_params(opening_span.start);
                let loc = self.fixed_loc(opening_span.start);
                self.set_loc(params, loc);
                self.add(
                    Kind::LambdaVar {
                        params,
                        locals: vec![],
                    },
                    loc,
                )
            }
        };
        // A lambda without parens but with empty params is located at the
        // opening.
        if let Kind::LambdaVar { params: p, .. } = self.kind(params).clone()
            && self.params_empty(p)
        {
            let loc = self.fixed_loc(opening_span.start);
            self.set_loc(p, loc);
            self.set_loc(params, loc);
        }

        let void_from = n
            .parameters()
            .map(|p| node_span(&p))
            .unwrap_or(opening_span);
        let statements = if braces {
            match n.body() {
                None => self.statements(vec![], opening_span.end),
                Some(body) => match body.as_statements_node() {
                    Some(stmts) => self.statements_with_void(&stmts, void_from, false),
                    None => {
                        let b = self.body_node(Some(body), opening_span, false, opening_span.end);
                        match self.kind(b) {
                            Kind::BodyStmt { statements, .. } => *statements,
                            _ => b,
                        }
                    }
                },
            }
        } else {
            self.body_node(n.body(), opening_span, false, opening_span.end)
        };

        let start_char = self.find_next_statement_start(opening_span.end);
        let closing = self.token_loc(closing_span);
        if braces {
            self.bind(statements, start_char, closing.start_char);
        } else {
            self.bind_bodystmt(statements, start_char, closing.start_char);
        }
        let tlambda = self.token_loc(span(&n.operator_loc()));
        let loc = tlambda.to(&closing);
        self.add(Kind::Lambda { params, statements }, loc)
    }

    // ------------------------------------------------------------------
    // Conditionals and loops
    // ------------------------------------------------------------------

    /// `if`, `elsif` (when `end_keyword` is given) and ternaries.
    fn if_node(&mut self, node: &pm::Node<'_>, end_keyword: Option<Span>) -> NodeId {
        let n = node.as_if_node().unwrap();
        if n.then_keyword_loc()
            .is_some_and(|t| self.slice(span(&t)) == "?")
        {
            let predicate = self.visit(&n.predicate());
            let truthy = self.visit(&n.statements().unwrap().body().iter().next().unwrap());
            let subsequent = n.subsequent().unwrap();
            let falsy = match subsequent.as_else_node() {
                Some(e) => self.visit(&e.statements().unwrap().body().iter().next().unwrap()),
                None => self.visit(&subsequent),
            };
            let loc = self.to(predicate, falsy);
            return self.add(
                Kind::IfOp {
                    predicate,
                    truthy,
                    falsy,
                },
                loc,
            );
        }

        let modifier = n
            .statements()
            .is_some_and(|s| node_span(&n.predicate()).start > span(&s.location()).start);
        if modifier {
            let statement = self.visit(&n.statements().unwrap().body().iter().next().unwrap());
            let predicate = self.visit(&n.predicate());
            let stmt_loc = self.loc(statement);
            let statements = self.add(
                Kind::Statements {
                    body: vec![statement],
                },
                stmt_loc,
            );
            let loc = self.to(statement, predicate);
            return self.add(
                Kind::IfNode {
                    predicate,
                    statements,
                    consequent: None,
                },
                loc,
            );
        }

        let end_span = end_keyword.or_else(|| n.end_keyword_loc().map(|l| span(&l)));
        let predicate = self.visit(&n.predicate());
        let statements = match n.statements() {
            None => self.statements(vec![], self.loc(predicate).end_char),
            Some(stmts) => {
                let body: Vec<NodeId> = stmts.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, self.loc(predicate).end_char)
            }
        };
        let consequent = n
            .subsequent()
            .map(|sub| self.conditional_subsequent(&sub, end_span));
        let keyword_span = span(&n.if_keyword_loc().unwrap());
        let elsif = self.slice(keyword_span) == "elsif";
        let beginning = self.token_loc(keyword_span);
        let ending = match consequent {
            Some(c) => self.loc(c),
            None => self.token_loc(end_span.unwrap()),
        };
        let then_limit = if elsif {
            let stmts = self.loc(statements).start_char;
            if stmts > self.loc(predicate).end_char {
                stmts
            } else {
                ending.start_char
            }
        } else {
            ending.start_char
        };
        let then_keyword = self.consume_then_between(self.loc(predicate).end_char, then_limit);
        let after = match then_keyword {
            Some(t) => t.end,
            None if elsif => {
                // find_keyword_between(:then) || find_token_between(Semicolon)
                self.semicolon_between(
                    self.loc(predicate).end_char,
                    self.loc(statements).start_char,
                )
                .map_or(self.loc(predicate).end_char, |s| s + 1)
            }
            None => self.loc(predicate).end_char,
        };
        let start_char = self.find_next_statement_start(after);
        self.bind(statements, start_char, ending.start_char);
        let loc = beginning.to(&ending);
        if elsif {
            self.add(
                Kind::Elsif {
                    predicate,
                    statements,
                    consequent,
                },
                loc,
            )
        } else {
            self.add(
                Kind::IfNode {
                    predicate,
                    statements,
                    consequent,
                },
                loc,
            )
        }
    }

    fn conditional_subsequent(&mut self, node: &pm::Node<'_>, end_span: Option<Span>) -> NodeId {
        if let Some(e) = node.as_else_node() {
            let end = e.end_keyword_loc().map(|l| span(&l)).or(end_span);
            return self.else_node(&e, end);
        }
        self.if_node(node, end_span)
    }

    /// `on_else` for if/unless/case.
    fn else_node(&mut self, node: &pm::ElseNode<'_>, end_span: Option<Span>) -> NodeId {
        let keyword_span = span(&node.else_keyword_loc());
        let statements = match node.statements() {
            None => self.statements(vec![], keyword_span.end),
            Some(stmts) => self.statements_with_void(&stmts, keyword_span, false),
        };
        let keyword = self.kw(keyword_span);
        let ending = self.token_loc(end_span.unwrap_or(keyword_span));
        let start_char = self.find_next_statement_start(keyword_span.end);
        self.bind(statements, start_char, ending.start_char);
        let loc = self.loc(keyword).to(&ending);
        self.add(
            Kind::Else {
                keyword,
                statements,
            },
            loc,
        )
    }

    fn unless_node(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_unless_node().unwrap();
        let modifier = n
            .statements()
            .is_some_and(|s| node_span(&n.predicate()).start > span(&s.location()).start);
        if modifier {
            let statement = self.visit(&n.statements().unwrap().body().iter().next().unwrap());
            let predicate = self.visit(&n.predicate());
            let stmt_loc = self.loc(statement);
            let statements = self.add(
                Kind::Statements {
                    body: vec![statement],
                },
                stmt_loc,
            );
            let loc = self.to(statement, predicate);
            return self.add(
                Kind::UnlessNode {
                    predicate,
                    statements,
                    consequent: None,
                },
                loc,
            );
        }
        let end_span = n.end_keyword_loc().map(|l| span(&l));
        let predicate = self.visit(&n.predicate());
        let statements = match n.statements() {
            None => self.statements(vec![], self.loc(predicate).end_char),
            Some(stmts) => {
                let body: Vec<NodeId> = stmts.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, self.loc(predicate).end_char)
            }
        };
        let consequent = n.else_clause().map(|e| self.else_node(&e, end_span));
        let beginning = self.token_loc(span(&n.unless_keyword_loc()));
        let ending = match consequent {
            Some(c) => self.loc(c),
            None => self.token_loc(end_span.unwrap()),
        };
        let after = self
            .consume_then_between(self.loc(predicate).end_char, ending.start_char)
            .map_or(self.loc(predicate).end_char, |t| t.end);
        let start_char = self.find_next_statement_start(after);
        self.bind(statements, start_char, ending.start_char);
        let loc = beginning.to(&ending);
        self.add(
            Kind::UnlessNode {
                predicate,
                statements,
                consequent,
            },
            loc,
        )
    }

    /// The last `;` token between two offsets, skipping comments.
    fn semicolon_between(&self, start: usize, end: usize) -> Option<usize> {
        let mut found = None;
        let mut i = start;
        while i < end.min(self.bytes.len()) {
            match self.bytes[i] {
                b';' => found = Some(i),
                b'#' => {
                    while i < end && self.bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        found
    }

    fn loop_node(&mut self, node: &pm::Node<'_>, is_while: bool) -> NodeId {
        let (keyword_loc, do_loc, closing_loc, predicate, stmts) = if is_while {
            let n = node.as_while_node().unwrap();
            (
                span(&n.while_keyword_loc()),
                n.do_keyword_loc().map(|l| span(&l)),
                n.end_keyword_loc().map(|l| span(&l)),
                n.predicate(),
                n.statements(),
            )
        } else {
            let n = node.as_until_node().unwrap();
            (
                span(&n.until_keyword_loc()),
                n.do_keyword_loc().map(|l| span(&l)),
                n.end_keyword_loc().map(|l| span(&l)),
                n.predicate(),
                n.statements(),
            )
        };
        let modifier = stmts
            .as_ref()
            .is_some_and(|s| node_span(&predicate).start > span(&s.location()).start);
        let make = |predicate, statements| {
            if is_while {
                Kind::WhileNode {
                    predicate,
                    statements,
                }
            } else {
                Kind::UntilNode {
                    predicate,
                    statements,
                }
            }
        };
        if modifier {
            let statement = self.visit(&stmts.unwrap().body().iter().next().unwrap());
            let predicate = self.visit(&predicate);
            let stmt_loc = self.loc(statement);
            let statements = self.add(
                Kind::Statements {
                    body: vec![statement],
                },
                stmt_loc,
            );
            let loc = self.to(statement, predicate);
            return self.add(make(predicate, statements), loc);
        }
        let predicate = self.visit(&predicate);
        let statements = match stmts {
            None => self.statements(vec![], self.loc(predicate).end_char),
            Some(s) => {
                let body: Vec<NodeId> = s.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, self.loc(predicate).end_char)
            }
        };
        let keyword = self.token_loc(keyword_loc);
        let ending = self.token_loc(closing_loc.unwrap());
        let delimiter_end = match do_loc {
            Some(d) => Some(d.end),
            None => {
                let stmt_start = self.loc(statements).start_char;
                let limit = if stmt_start > self.loc(predicate).end_char {
                    stmt_start
                } else {
                    ending.start_char
                };
                self.semicolon_between(self.loc(predicate).end_char, limit)
                    .map(|s| s + 1)
            }
        };
        let after = delimiter_end.unwrap_or(self.loc(predicate).end_char);
        let start_char = self.find_next_statement_start(after);
        self.bind(statements, start_char, ending.start_char);
        let loc = keyword.to(&ending);
        self.add(make(predicate, statements), loc)
    }

    fn for_loop(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_for_node().unwrap();
        let index = self.visit(&n.index());
        let collection = self.visit(&n.collection());
        let statements = match n.statements() {
            None => self.statements(vec![], self.loc(collection).end_char),
            Some(s) => {
                let body: Vec<NodeId> = s.body().iter().map(|s| self.visit(&s)).collect();
                self.statements(body, self.loc(collection).end_char)
            }
        };
        let beginning = self.token_loc(span(&n.for_keyword_loc()));
        let in_keyword = span(&n.in_keyword_loc());
        let ending = self.token_loc(span(&n.end_keyword_loc()));
        let delimiter_end = match n.do_keyword_loc() {
            Some(d) => Some(span(&d).end),
            None => self
                .semicolon_between(self.loc(collection).end_char, ending.start_char)
                .map(|s| s + 1),
        };
        let after = delimiter_end.unwrap_or(self.loc(collection).end_char);
        let start_char = self.find_next_statement_start(after);
        self.bind(statements, start_char, ending.start_char);
        if matches!(self.kind(index), Kind::MLHS { .. }) {
            let between = Span {
                start: self.loc(index).end_char,
                end: in_keyword.start,
            };
            if between.start <= between.end && self.slice(between).trim().starts_with(',') {
                self.set_comma(index);
            }
        }
        let loc = beginning.to(&ending);
        self.add(
            Kind::For {
                index,
                collection,
                statements,
            },
            loc,
        )
    }

    fn case(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_case_node().unwrap();
        let value = n.predicate().map(|p| self.visit(&p));
        let end_span = span(&n.end_keyword_loc());
        let conditions: Vec<pm::Node> = n.conditions().iter().collect();
        // Ripper parses the clauses in order but reduces them last to first.
        let mut clauses = Vec::new();
        for condition in &conditions {
            let w = condition.as_when_node().unwrap();
            let elements: Vec<pm::Node> = w.conditions().iter().collect();
            let arguments = self.arguments_list(&elements, span(&w.when_keyword_loc()).end);
            let statements = match w.statements() {
                None => self.statements(vec![], self.loc(arguments).end_char),
                Some(s) => {
                    let body: Vec<NodeId> = s.body().iter().map(|s| self.visit(&s)).collect();
                    self.statements(body, self.loc(arguments).end_char)
                }
            };
            clauses.push((span(&w.when_keyword_loc()), arguments, statements));
        }
        let mut current = n.else_clause().map(|e| self.else_node(&e, Some(end_span)));
        for (keyword, arguments, statements) in clauses.into_iter().rev() {
            let beginning = self.token_loc(keyword);
            let ending = match current {
                Some(c) => self.loc(c),
                None => self.token_loc(end_span),
            };
            // on_when takes the last unconsumed `then` anywhere before it,
            // which can belong to an earlier clause.
            let after = match self.consume_then_before(ending.start_char) {
                Some(t) => t.end,
                None => self.loc(arguments).end_char,
            };
            let start_char = self.find_next_statement_start(after);
            self.bind(statements, start_char, ending.start_char);
            let loc = beginning.to(&ending);
            current = Some(self.add(
                Kind::When {
                    arguments,
                    statements,
                    consequent: current,
                },
                loc,
            ));
        }
        let consequent = current.unwrap();
        let keyword = self.kw(span(&n.case_keyword_loc()));
        let loc = self.to(keyword, consequent);
        self.add(
            Kind::Case {
                keyword,
                value,
                consequent,
            },
            loc,
        )
    }

    fn case_match(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_case_match_node().unwrap();
        let value = n.predicate().map(|p| self.visit(&p));
        let end_span = span(&n.end_keyword_loc());
        let conditions: Vec<pm::Node> = n.conditions().iter().collect();
        // Patterns and bodies are parsed in order; the clauses reduce last
        // to first.
        let mut clauses = Vec::new();
        for condition in &conditions {
            let i = condition.as_in_node().unwrap();
            let pattern = self.visit_pattern(&i.pattern());
            let statements = match i.statements() {
                None => self.statements(vec![], self.loc(pattern).end_char),
                Some(s) => {
                    let body: Vec<NodeId> = s.body().iter().map(|s| self.visit(&s)).collect();
                    self.statements(body, self.loc(pattern).end_char)
                }
            };
            clauses.push((span(&i.in_keyword_loc()), pattern, statements));
        }
        let mut current = n.else_clause().map(|e| self.else_node(&e, Some(end_span)));
        for (in_loc, pattern, statements) in clauses.into_iter().rev() {
            let beginning = self.token_loc(in_loc);
            let ending = match current {
                Some(c) => self.loc(c),
                None => self.token_loc(end_span),
            };
            let stmts_start = self.loc(statements).start_char;
            let limit = if stmts_start > self.loc(pattern).end_char {
                stmts_start
            } else {
                ending.start_char
            };
            let then = self.consume_then_between(self.loc(pattern).end_char, limit);
            let after = match then {
                Some(t) => t.end,
                None => self.loc(pattern).end_char,
            };
            let mut start_char = self.find_next_statement_start(after);
            if self.bytes.get(start_char) == Some(&b')') {
                start_char = self.find_next_statement_start(start_char + 1);
            }
            self.bind(statements, start_char, ending.start_char);
            let loc = beginning.to(&ending);
            current = Some(self.add(
                Kind::In {
                    pattern,
                    statements,
                    consequent: current,
                },
                loc,
            ));
        }
        let consequent = current.unwrap();
        let keyword = self.kw(span(&n.case_keyword_loc()));
        let loc = self.to(keyword, consequent);
        self.add(
            Kind::Case {
                keyword,
                value,
                consequent,
            },
            loc,
        )
    }

    // ------------------------------------------------------------------
    // Patterns
    // ------------------------------------------------------------------

    fn visit_pattern(&mut self, node: &pm::Node<'_>) -> NodeId {
        // `in pattern if guard`: Ripper's if_mod around the pattern.
        let guard = match node {
            pm::Node::IfNode { .. } => {
                let n = node.as_if_node().unwrap();
                Some((n.predicate(), n.statements(), true))
            }
            pm::Node::UnlessNode { .. } => {
                let n = node.as_unless_node().unwrap();
                Some((n.predicate(), n.statements(), false))
            }
            _ => None,
        };
        if let Some((predicate, Some(statements), is_if)) = guard
            && node_span(&predicate).start > span(&statements.location()).start
        {
            let first = statements.body().iter().next().unwrap();
            let statement = self.visit_pattern(&first);
            let predicate = self.visit(&predicate);
            let stmt_loc = self.loc(statement);
            let statements = self.add(
                Kind::Statements {
                    body: vec![statement],
                },
                stmt_loc,
            );
            let loc = self.to(statement, predicate);
            let kind = if is_if {
                Kind::IfNode {
                    predicate,
                    statements,
                    consequent: None,
                }
            } else {
                Kind::UnlessNode {
                    predicate,
                    statements,
                    consequent: None,
                }
            };
            return self.add(kind, loc);
        }
        if let Some(paren) = node.as_parentheses_node()
            && let Some(body) = paren.body()
        {
            if let Some(stmts) = body.as_statements_node()
                && let Some(first) = stmts.body().iter().next()
            {
                return self.visit_pattern(&first);
            }
            return self.visit_pattern(&body);
        }
        self.visit(node)
    }

    fn pattern_constant(&mut self, node: Option<pm::Node<'_>>) -> Option<NodeId> {
        node.map(|c| self.visit(&c))
    }

    fn splat_var_field(&mut self, splat: &pm::SplatNode<'_>, include_star: bool) -> NodeId {
        let star = self.token_loc(span(&splat.operator_loc()));
        match splat.expression() {
            None => self.add(Kind::VarField { value: NilOr::None }, star),
            Some(expression) => {
                let field = self.visit(&expression);
                if include_star {
                    let loc = star.to(&self.loc(field));
                    self.set_loc(field, loc);
                }
                field
            }
        }
    }

    fn array_pattern(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_array_pattern_node().unwrap();
        let constant = self.pattern_constant(n.constant());
        let requireds: Vec<NodeId> = n
            .requireds()
            .iter()
            .map(|r| self.visit_pattern(&r))
            .collect();
        let rest = n.rest().map(|r| match r.as_splat_node() {
            Some(splat) => self.splat_var_field(&splat, false),
            None => {
                // ImplicitRestNode: `in [a,]`
                let loc = self.token_loc(node_span(&r));
                self.add(Kind::VarField { value: NilOr::None }, loc)
            }
        });
        let posts: Vec<NodeId> = n.posts().iter().map(|p| self.visit_pattern(&p)).collect();
        let mut locs: Vec<Location> = Vec::new();
        if let Some(c) = constant {
            locs.push(self.loc(c));
        }
        if let Some(o) = n.opening_loc() {
            locs.push(self.token_loc(span(&o)));
        }
        for &r in &requireds {
            locs.push(self.loc(r));
        }
        if let Some(r) = rest {
            locs.push(self.loc(r));
        }
        for &p in &posts {
            locs.push(self.loc(p));
        }
        if let Some(c) = n.closing_loc() {
            locs.push(self.token_loc(span(&c)));
        }
        let loc = match (locs.first(), locs.last()) {
            (Some(f), Some(l)) => f.to(l),
            _ => self.token_loc(node_span(node)),
        };
        self.add(
            Kind::AryPtn {
                constant,
                requireds,
                rest,
                posts,
            },
            loc,
        )
    }

    fn find_pattern(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_find_pattern_node().unwrap();
        let constant = self.pattern_constant(n.constant());
        let left = self.splat_var_field(&n.left(), true);
        let values: Vec<NodeId> = n
            .requireds()
            .iter()
            .map(|r| self.visit_pattern(&r))
            .collect();
        let right = self.splat_var_field(&n.right(), true);
        let opening = n.opening_loc().map(|o| self.token_loc(span(&o)));
        let closing = n.closing_loc().map(|c| self.token_loc(span(&c)));
        let start = match constant {
            Some(c) => self.loc(c),
            None => opening.unwrap_or(self.loc(left)),
        };
        let end = closing.unwrap_or(self.loc(right));
        let loc = start.to(&end);
        self.add(
            Kind::FndPtn {
                constant,
                left,
                values,
                right,
            },
            loc,
        )
    }

    fn hash_pattern(&mut self, node: &pm::Node<'_>) -> NodeId {
        let n = node.as_hash_pattern_node().unwrap();
        let constant = self.pattern_constant(n.constant());
        let mut keywords = Vec::new();
        let mut locs: Vec<Location> = Vec::new();
        if let Some(c) = constant {
            locs.push(self.loc(c));
        }
        for element in n.elements().iter() {
            let assoc = element.as_assoc_node().unwrap();
            let key = self.visit(&assoc.key());
            let value = match assoc.value() {
                pm::Node::ImplicitNode { .. } => None,
                v => Some(self.visit_pattern(&v)),
            };
            locs.push(self.loc(key));
            if let Some(v) = value {
                locs.push(self.loc(v));
            }
            keywords.push((key, value));
        }
        let pattern_end = node_span(node).end;
        let mut keyword_rest = n.rest().map(|rest| {
            self.consume_pow_before(pattern_end);
            if let Some(splat) = rest.as_assoc_splat_node() {
                let star = self.token_loc(span(&splat.operator_loc()));
                match splat.value() {
                    Some(v) => self.visit(&v),
                    None => self.add(Kind::VarField { value: NilOr::None }, star),
                }
            } else {
                // **nil
                let loc = self.token_loc(node_span(&rest));
                self.add(
                    Kind::VarField {
                        value: NilOr::NilSymbol,
                    },
                    loc,
                )
            }
        });
        if keyword_rest.is_none()
            && let Some(pos) = self.consume_pow_before(pattern_end)
        {
            // on_hshptn grabs any `**` operator token still in the list
            // (e.g. from `a**b`, which on_binary doesn't consume) and
            // invents a bare `**` rest.
            //
            // This changes the program, and is kept on purpose: oxtree keeps
            // byte-parity with syntax_tree 6.3.0, bugs included (user
            // decision, see docs/OXTREE.md "Known gaps").
            let loc = self.token_loc(Span {
                start: pos,
                end: pos + 2,
            });
            keyword_rest = Some(self.add(Kind::VarField { value: NilOr::None }, loc));
        }
        if let Some(r) = keyword_rest {
            locs.push(self.loc(r));
        }
        if constant.is_none()
            && let (Some(o), Some(c)) = (n.opening_loc(), n.closing_loc())
        {
            locs.insert(0, self.token_loc(span(&o)));
            locs.push(self.token_loc(span(&c)));
        }
        let loc = match (locs.first(), locs.last()) {
            (Some(f), Some(l)) => f.to(l),
            _ => self.token_loc(node_span(node)),
        };
        self.add(
            Kind::HshPtn {
                constant,
                keywords,
                keyword_rest,
            },
            loc,
        )
    }
}

enum HeredocItem<'pr> {
    Content(Span),
    Embed(pm::Node<'pr>),
}

impl HeredocItem<'_> {
    fn bounds(&self) -> (usize, usize) {
        match self {
            HeredocItem::Content(s) => (s.start, s.end),
            HeredocItem::Embed(node) => {
                let s = node_span(node);
                (s.start, s.end)
            }
        }
    }
}

fn collect_heredoc_items<'pr>(part: pm::Node<'pr>, items: &mut Vec<HeredocItem<'pr>>) {
    if let Some(string) = part.as_string_node() {
        items.push(HeredocItem::Content(span(&string.content_loc())));
    } else if let Some(interp) = part.as_interpolated_string_node() {
        for inner in interp.parts().iter() {
            collect_heredoc_items(inner, items);
        }
    } else {
        items.push(HeredocItem::Embed(part));
    }
}

/// Collects the offsets of the tokens whose consumption by Ripper event
/// handlers has to be simulated: `then` keywords and `**` operators.
#[derive(Default)]
struct TokenCollector {
    then: Vec<usize>,
    pow: Vec<usize>,
}

impl TokenCollector {
    fn collect(&mut self, node: &pm::Node<'_>) {
        let then = match node {
            pm::Node::IfNode { .. } => node.as_if_node().unwrap().then_keyword_loc(),
            pm::Node::UnlessNode { .. } => node.as_unless_node().unwrap().then_keyword_loc(),
            pm::Node::WhenNode { .. } => node.as_when_node().unwrap().then_keyword_loc(),
            pm::Node::InNode { .. } => node.as_in_node().unwrap().then_keyword_loc(),
            pm::Node::RescueNode { .. } => node.as_rescue_node().unwrap().then_keyword_loc(),
            _ => None,
        };
        if let Some(then) = then
            && then.as_slice() == b"then"
        {
            self.then.push(then.start() as usize);
        }
        let pow = match node {
            pm::Node::CallNode { .. } => {
                let call = node.as_call_node().unwrap();
                if call.name().as_slice() == b"**" && call.call_operator_loc().is_none() {
                    call.message_loc()
                } else {
                    None
                }
            }
            pm::Node::AssocSplatNode { .. } => {
                Some(node.as_assoc_splat_node().unwrap().operator_loc())
            }
            pm::Node::KeywordRestParameterNode { .. } => Some(
                node.as_keyword_rest_parameter_node()
                    .unwrap()
                    .operator_loc(),
            ),
            pm::Node::NoKeywordsParameterNode { .. } => {
                Some(node.as_no_keywords_parameter_node().unwrap().operator_loc())
            }
            _ => None,
        };
        if let Some(pow) = pow {
            self.pow.push(pow.start() as usize);
        }
    }
}

impl<'pr> pm::Visit<'pr> for TokenCollector {
    fn visit_branch_node_enter(&mut self, node: pm::Node<'pr>) {
        self.collect(&node);
    }

    fn visit_leaf_node_enter(&mut self, node: pm::Node<'pr>) {
        self.collect(&node);
    }
}
