//! Port of `lib/rubocop/ext/regexp_parser.rb`: source locations for
//! regexp_parser expressions (`expression`, `loc.body`, `loc.quantifier`,
//! and `loc.begin`/`loc.end` for character sets). Expression positions are
//! characters from the regexp's `origin` (the end of its opening
//! delimiter); ranges are byte offsets into the file.

use crate::processed_source::Range;
use crate::regexp_parser::{Class, ExprId, Expression, Tree};

/// A parsed regexp and where it sits in the source.
pub struct ParsedTree<'a> {
    pub tree: Tree,
    source: &'a str,
    origin: usize,
}

impl<'a> ParsedTree<'a> {
    pub fn new(tree: Tree, source: &'a str, origin: usize) -> Self {
        Self {
            tree,
            source,
            origin,
        }
    }

    pub fn get(&self, id: ExprId) -> &Expression {
        self.tree.get(id)
    }

    /// The byte offset `chars` characters after the origin.
    fn pos(&self, chars: usize) -> usize {
        self.source[self.origin..]
            .char_indices()
            .nth(chars)
            .map_or(self.source.len(), |(i, _)| self.origin + i)
    }

    fn range(&self, begin: usize, end: usize) -> Range {
        Range::new(self.pos(begin), self.pos(end))
    }

    /// `expression`: the expression with its quantifier.
    pub fn expression(&self, id: ExprId) -> Range {
        let ts = self.tree.ts(id);
        self.range(ts, ts + self.tree.full_length(id))
    }

    /// `loc.body`: the expression without its quantifier.
    pub fn body(&self, id: ExprId) -> Range {
        let ts = self.tree.ts(id);
        let mut end = ts + self.tree.full_length(id);
        if let Some(q) = &self.get(id).quantifier {
            end -= q.text.chars().count();
        }
        self.range(ts, end)
    }

    /// `loc.quantifier`
    pub fn quantifier(&self, id: ExprId) -> Option<Range> {
        let q = self.get(id).quantifier.as_ref()?;
        Some(self.range(q.ts, q.ts + q.text.chars().count()))
    }

    /// `loc.begin` of a character set (its `[`).
    pub fn set_begin(&self, id: ExprId) -> Option<Range> {
        (self.get(id).class == Class::CharacterSet).then(|| {
            let body = self.body(id);
            Range::new(body.begin_pos, body.begin_pos + 1)
        })
    }

    /// `loc.end` of a character set (its `]`).
    pub fn set_end(&self, id: ExprId) -> Option<Range> {
        (self.get(id).class == Class::CharacterSet).then(|| {
            let body = self.body(id);
            Range::new(body.end_pos - 1, body.end_pos)
        })
    }
}
