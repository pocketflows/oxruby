//! Port of `Parser::Source::Comment::Associator` (`associate_by_identity`):
//! attaches each comment to the node it documents.

use std::collections::HashMap;

use crate::ast::{MapKind, Node, NodeId, NodeType};
use crate::processed_source::{Comment, ProcessedSource};

struct Associator<'p, 'src> {
    comments: &'p [Comment<'src>],
    processed_source: &'p ProcessedSource<'src>,
    current: usize,
    mapping: HashMap<NodeId, Vec<usize>>,
}

impl Associator<'_, '_> {
    fn current_comment(&self) -> Option<&Comment<'_>> {
        self.comments.get(self.current)
    }

    fn comment_line(&self) -> usize {
        self.processed_source
            .line_of(self.comments[self.current].range.begin_pos)
    }

    fn children_in_source_order<'a>(node: Node<'a>) -> Vec<Node<'a>> {
        use NodeType::*;
        let mut children: Vec<Node<'a>> = node.child_nodes().filter(|c| c.has_source()).collect();
        if matches!(
            node.ty(),
            If | While | WhilePost | Until | UntilPost | Masgn
        ) {
            children.sort_by_key(|c| c.source_range().begin_pos);
        }
        children
    }

    fn visit(&mut self, node: Node<'_>) {
        self.process_leading_comments(node);
        if self.current_comment().is_none() {
            return;
        }
        if self.comment_line() <= node.last_line() || node.loc().kind == MapKind::Heredoc {
            for child in Self::children_in_source_order(node) {
                self.visit(child);
            }
            self.process_trailing_comments(node);
        }
    }

    fn process_leading_comments(&mut self, node: Node<'_>) {
        if node.is(NodeType::Begin) {
            return;
        }
        while let Some(comment) = self.current_comment() {
            if comment.range.end_pos <= node.source_range().begin_pos {
                self.associate_and_advance(node);
            } else {
                break;
            }
        }
    }

    fn process_trailing_comments(&mut self, node: Node<'_>) {
        while let Some(comment) = self.current_comment() {
            if comment.range.end_pos <= node.source_range().end_pos {
                self.associate_and_advance(node);
            } else {
                break;
            }
        }
        while self.current_comment().is_some() && self.comment_line() == node.last_line() {
            self.associate_and_advance(node);
        }
    }

    fn associate_and_advance(&mut self, node: Node<'_>) {
        self.mapping
            .entry(node.id())
            .or_default()
            .push(self.current);
        self.current += 1;
    }

    fn advance_through_directives(&mut self) {
        static MAGIC_COMMENT_RE: std::sync::LazyLock<onig::Regex> =
            std::sync::LazyLock::new(|| {
                onig::Regex::new(
                    r"^#\s*(-\*-|)\s*(frozen_string_literal|warn_indent|warn_past_scope):.*\1$",
                )
                .unwrap()
            });
        static ENCODING_RE: std::sync::LazyLock<onig::Regex> = std::sync::LazyLock::new(|| {
            onig::Regex::new(r"[\s#](en)?coding\s*[:=]\s*([A-Za-z0-9_-]+)").unwrap()
        });
        if self
            .current_comment()
            .is_some_and(|c| c.text.starts_with("#!"))
        {
            self.current += 1;
        }
        if self
            .current_comment()
            .is_some_and(|c| MAGIC_COMMENT_RE.find(c.text).is_some())
        {
            self.current += 1;
        }
        if self
            .current_comment()
            .is_some_and(|c| ENCODING_RE.find(c.text).is_some())
        {
            self.current += 1;
        }
    }
}

pub fn associate(
    ast: Option<Node<'_>>,
    comments: &[Comment<'_>],
    processed_source: &ProcessedSource<'_>,
) -> HashMap<NodeId, Vec<usize>> {
    let mut associator = Associator {
        comments,
        processed_source,
        current: 0,
        mapping: HashMap::new(),
    };
    associator.advance_through_directives();
    if let Some(ast) = ast {
        associator.visit(ast);
    }
    associator.mapping
}
