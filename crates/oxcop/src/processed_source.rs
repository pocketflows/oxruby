//! Port of `RuboCop::AST::ProcessedSource` (from rubocop-ast), backed by
//! Prism instead of the `parser` gem.
//!
//! Like `Parser::Source::Buffer`, positions refer to the source with `\r\n`
//! normalized to `\n` (`buffer_source`); `raw_source` is the original text.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use ruby_prism::ParseResult;

use crate::ast::Ast;
use crate::ast::lexer::Token;

/// A half-open byte range into the source, like `Parser::Source::Range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Range {
    pub begin_pos: usize,
    pub end_pos: usize,
}

impl Range {
    pub fn new(begin_pos: usize, end_pos: usize) -> Self {
        Self { begin_pos, end_pos }
    }

    pub fn len(&self) -> usize {
        self.end_pos - self.begin_pos
    }

    pub fn is_empty(&self) -> bool {
        self.begin_pos == self.end_pos
    }

    /// `Range#join`.
    pub fn join(&self, other: Range) -> Range {
        Range::new(
            self.begin_pos.min(other.begin_pos),
            self.end_pos.max(other.end_pos),
        )
    }

    /// `Range#begin` (an empty range at the start).
    pub fn begin(&self) -> Range {
        Range::new(self.begin_pos, self.begin_pos)
    }

    /// `Range#end` (an empty range at the end).
    pub fn end(&self) -> Range {
        Range::new(self.end_pos, self.end_pos)
    }

    pub fn with_begin(&self, begin_pos: usize) -> Range {
        Range::new(begin_pos, self.end_pos)
    }

    pub fn with_end(&self, end_pos: usize) -> Range {
        Range::new(self.begin_pos, end_pos)
    }

    /// `Range#adjust`.
    pub fn adjust(&self, begin: isize, end: isize) -> Range {
        Range::new(
            (self.begin_pos as isize + begin) as usize,
            (self.end_pos as isize + end) as usize,
        )
    }

    /// `Range#resize`.
    pub fn resize(&self, size: usize) -> Range {
        Range::new(self.begin_pos, self.begin_pos + size)
    }

    fn cmp_sign(a: usize, b: usize) -> i32 {
        match a.cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }

    /// `Range#contains?`: strictly contains `other`.
    pub fn contains(&self, other: Range) -> bool {
        Self::cmp_sign(other.begin_pos, self.begin_pos)
            + Self::cmp_sign(self.end_pos, other.end_pos)
            >= if other.is_empty() { 2 } else { 1 }
    }

    /// `Range#disjoint?`.
    pub fn disjoint(&self, other: Range) -> bool {
        if self.is_empty() && other.is_empty() {
            self.begin_pos != other.begin_pos
        } else {
            self.begin_pos >= other.end_pos || other.begin_pos >= self.end_pos
        }
    }

    /// `Range#overlaps?`.
    pub fn overlaps(&self, other: Range) -> bool {
        !self.disjoint(other)
    }

    /// `Range#crossing?`.
    pub fn crossing(&self, other: Range) -> bool {
        self.overlaps(other)
            && Self::cmp_sign(self.begin_pos, other.begin_pos)
                * Self::cmp_sign(self.end_pos, other.end_pos)
                == 1
    }

    /// `Range#intersect`.
    pub fn intersect(&self, other: Range) -> Option<Range> {
        let begin = self.begin_pos.max(other.begin_pos);
        let end = self.end_pos.min(other.end_pos);
        (begin <= end).then(|| Range::new(begin, end))
    }
}

impl From<&ruby_prism::Location<'_>> for Range {
    fn from(location: &ruby_prism::Location<'_>) -> Self {
        Self::new(location.start() as usize, location.end() as usize)
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: Range,
}

/// A `Parser::Source::Comment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment<'src> {
    pub range: Range,
    pub text: &'src str,
}

impl Comment<'_> {
    /// `Comment#document?` (`=begin`/`=end`).
    pub fn is_document(&self) -> bool {
        self.text.starts_with("=begin")
    }
}

/// `source.gsub("\r\n", "\n")`, as `Parser::Source::Buffer` does.
pub fn normalize_newlines(source: &str) -> Cow<'_, str> {
    if source.contains("\r\n") {
        Cow::Owned(source.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(source)
    }
}

pub struct ProcessedSource<'src> {
    path: PathBuf,
    raw_source: &'src str,
    source: &'src str,
    parse_result: ParseResult<'src>,
    line_starts: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
    lines: Vec<&'src str>,
    ruby_version: f64,
    ast: Ast,
    comments: Vec<Comment<'src>>,
    tokens: OnceCell<Vec<Token>>,
    sorted_tokens: OnceCell<Vec<Token>>,
    comment_index: OnceCell<std::collections::HashMap<usize, usize>>,
    ast_with_comments: OnceCell<std::collections::HashMap<crate::ast::NodeId, Vec<usize>>>,
}

impl<'src> ProcessedSource<'src> {
    /// `raw_source` is the file as read; `source` the same with `\r\n`
    /// normalized (see [`normalize_newlines`]).
    pub fn new(raw_source: &'src str, source: &'src str, ruby_version: f64, path: &Path) -> Self {
        let parse_result = crate::prism::parse(source, ruby_version);
        let diagnostics: Vec<Diagnostic> = parse_result
            .errors()
            .map(|d| Diagnostic {
                message: String::from_utf8_lossy(d.message()).into_owned(),
                range: Range::from(&d.location()),
            })
            // Below Ruby 3.3 RuboCop parses with the `parser` gem, which
            // accepts `retry` outside of `rescue` (Ruby rejects it when
            // compiling, not parsing).
            .filter(|d| !(ruby_version < 3.3 && d.message == "Invalid retry without rescue"))
            .collect();

        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));

        // `ProcessedSource#lines` stops at `__END__`.
        let end_line = parse_result
            .data_loc()
            .map(|loc| line_index(&line_starts, loc.start() as usize));
        let mut lines: Vec<&str> = source.split('\n').collect();
        if let Some(end_line) = end_line {
            lines.truncate(end_line);
        }

        let comments = parse_result
            .comments()
            .map(|c| {
                let range = Range::from(&c.location());
                Comment {
                    range,
                    text: &source[range.begin_pos..range.end_pos],
                }
            })
            .collect();

        let ast = if diagnostics.is_empty() {
            let file_name = path.to_string_lossy();
            Ast::from_prism(&parse_result, source, &file_name)
        } else {
            Ast::default()
        };

        Self {
            ast,
            path: path.to_path_buf(),
            raw_source,
            source,
            parse_result,
            line_starts,
            diagnostics,
            lines,
            ruby_version,
            comments,
            tokens: OnceCell::new(),
            sorted_tokens: OnceCell::new(),
            comment_index: OnceCell::new(),
            ast_with_comments: OnceCell::new(),
        }
    }

    /// `file_path` (`buffer.name`).
    pub fn file_path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    /// `blank?`: no AST.
    pub fn is_blank(&self) -> bool {
        self.ast.is_empty()
    }

    /// `start_with?`: whether the first line starts with `string`.
    pub fn starts_with(&self, string: &str) -> bool {
        self.lines.first().is_some_and(|l| l.starts_with(string))
    }

    /// Line (1-based) to the index of the comment that starts on it (the last
    /// one wins, as in rubocop-ast's `comment_index`).
    fn comment_index(&self) -> &std::collections::HashMap<usize, usize> {
        self.comment_index.get_or_init(|| {
            self.comments
                .iter()
                .enumerate()
                .map(|(i, c)| (self.line_of(c.range.begin_pos), i))
                .collect()
        })
    }

    /// `comment_at_line(line)`.
    pub fn comment_at_line(&self, line: usize) -> Option<&Comment<'src>> {
        self.comment_index().get(&line).map(|&i| &self.comments[i])
    }

    /// `line_with_comment?(line)`.
    pub fn line_with_comment(&self, line: usize) -> bool {
        self.comment_index().contains_key(&line)
    }

    /// `each_comment_in_lines(first..=last)`.
    pub fn comments_in_lines(&self, first: usize, last: usize) -> Vec<&Comment<'src>> {
        (first..=last)
            .filter_map(|l| self.comment_at_line(l))
            .collect()
    }

    /// `contains_comment?(range)`.
    pub fn contains_comment(&self, range: Range) -> bool {
        let first = self.line_of(range.begin_pos);
        let last = self.line_of(range.end_pos);
        (first..=last).any(|l| self.line_with_comment(l))
    }

    /// `line_indentation(line)`: leading whitespace length.
    pub fn line_indentation(&self, line: usize) -> usize {
        self.lines.get(line - 1).map_or(0, |l| {
            l.chars()
                .take_while(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
                .count()
        })
    }

    /// `sorted_tokens`: tokens sorted by position (stable).
    pub fn sorted_tokens(&self) -> &[Token] {
        self.sorted_tokens.get_or_init(|| {
            let mut tokens = self.tokens().to_vec();
            tokens.sort_by_key(|t| t.range.begin_pos);
            tokens
        })
    }

    fn first_token_index(&self, range: Range) -> usize {
        self.sorted_tokens()
            .partition_point(|t| t.range.begin_pos < range.begin_pos)
    }

    fn last_token_index(&self, range: Range) -> Option<usize> {
        let idx = self
            .sorted_tokens()
            .partition_point(|t| t.range.end_pos <= range.end_pos);
        idx.checked_sub(1)
    }

    /// `tokens_within(range)`.
    pub fn tokens_within(&self, range: Range) -> &[Token] {
        let begin = self.first_token_index(range);
        match self.last_token_index(range) {
            Some(end) if end >= begin => &self.sorted_tokens()[begin..=end],
            _ => &[],
        }
    }

    /// `first_token_of(range)`.
    pub fn first_token_of(&self, range: Range) -> Option<&Token> {
        self.sorted_tokens().get(self.first_token_index(range))
    }

    /// `last_token_of(range)`.
    pub fn last_token_of(&self, range: Range) -> Option<&Token> {
        self.last_token_index(range)
            .and_then(|i| self.sorted_tokens().get(i))
    }

    /// `ast_with_comments`: comments associated with each node
    /// (`Parser::Source::Comment.associate_by_identity`).
    pub fn ast_with_comments(&self) -> &std::collections::HashMap<crate::ast::NodeId, Vec<usize>> {
        self.ast_with_comments.get_or_init(|| {
            crate::ast::comment_associator::associate(self.ast(), &self.comments, self)
        })
    }

    /// The comments associated with `node`.
    pub fn node_comments(&self, node: crate::ast::Node<'_>) -> Vec<&Comment<'src>> {
        self.ast_with_comments()
            .get(&node.id())
            .map(|ids| ids.iter().map(|&i| &self.comments[i]).collect())
            .unwrap_or_default()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The source as read, before newline normalization.
    pub fn raw_source(&self) -> &'src str {
        self.raw_source
    }

    /// `buffer.source`: what every position refers to.
    pub fn buffer_source(&self) -> &'src str {
        self.source
    }

    pub fn ruby_version(&self) -> f64 {
        self.ruby_version
    }

    pub fn prism_ast(&self) -> ruby_prism::Node<'_> {
        self.parse_result.node()
    }

    /// The `parser`-gem-shaped AST (`nil` in RuboCop for an empty file).
    pub fn ast(&self) -> Option<crate::ast::Node<'_>> {
        self.ast.root()
    }

    pub fn tree(&self) -> &Ast {
        &self.ast
    }

    pub fn parse_result(&self) -> &ParseResult<'src> {
        &self.parse_result
    }

    pub fn valid_syntax(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn comments(&self) -> &[Comment<'src>] {
        &self.comments
    }

    /// `processed_source.tokens`, in the `parser` gem's format.
    pub fn tokens(&self) -> &[Token] {
        self.tokens.get_or_init(|| {
            if !self.valid_syntax() {
                return Vec::new();
            }
            crate::ast::token_builder::tokens(self)
        })
    }

    /// Source lines without their terminators, as in RuboCop.
    pub fn lines(&self) -> &[&'src str] {
        &self.lines
    }

    /// The range of the given 1-based line, excluding its newline.
    pub fn line_range(&self, line: usize) -> Range {
        let begin = self.line_starts[line - 1];
        Range::new(
            begin,
            begin + self.lines.get(line - 1).map_or(0, |l| l.len()),
        )
    }

    /// The byte offset where a 1-based line starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts[(line - 1).min(self.line_starts.len() - 1)]
    }

    /// 1-based line number for a byte offset.
    pub fn line_of(&self, pos: usize) -> usize {
        line_index(&self.line_starts, pos) + 1
    }

    /// 0-based column in characters for a byte offset.
    pub fn column_of(&self, pos: usize) -> usize {
        let start = self.line_starts[line_index(&self.line_starts, pos)];
        self.source[start..pos].chars().count()
    }

    /// Full text of the 1-based line (without terminator).
    pub fn source_line(&self, line: usize) -> &'src str {
        let start = self.line_starts[line - 1];
        let rest = &self.source[start..];
        rest.split('\n').next().unwrap_or("")
    }

    pub fn slice(&self, range: Range) -> &'src str {
        &self.source[range.begin_pos..range.end_pos]
    }
}

fn line_index(line_starts: &[usize], pos: usize) -> usize {
    match line_starts.binary_search(&pos) {
        Ok(i) => i,
        Err(i) => i - 1,
    }
}
