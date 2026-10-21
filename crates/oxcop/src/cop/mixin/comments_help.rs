//! Port of `lib/rubocop/cop/mixin/comments_help.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::parentheses;
use crate::processed_source::{Comment, ProcessedSource, Range};

/// `source_range_with_comment(node)`.
pub fn source_range_with_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let begin_pos = begin_pos_with_comment(ps, node);
    let end_pos = end_position_for(ps, node);
    Range::new(begin_pos, end_pos)
}

fn end_position_for(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let end_line = ps.line_of(node.source_range().end_pos);
    ps.line_range(end_line).end_pos
}

fn begin_pos_with_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let first_comment = ps.node_comments(node).into_iter().next();
    match first_comment {
        Some(comment) if ps.line_of(comment.range.begin_pos) < node.first_line() => {
            start_line_position(ps, ps.line_of(comment.range.begin_pos))
        }
        _ => start_line_position(ps, node.first_line()),
    }
}

fn start_line_position(ps: &ProcessedSource<'_>, line: usize) -> usize {
    // `buffer.line_range(line).begin_pos - 1` (may be -1 for line 1).
    ps.line_start(line).saturating_sub(1)
}

/// `contains_comments?(node)`.
pub fn contains_comments(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    !comments_in_range(ps, node).is_empty()
}

/// `comments_in_range(node)`: comments on `node.line...find_end_line`.
pub fn comments_in_range<'s>(ps: &'s ProcessedSource<'_>, node: Node<'_>) -> Vec<&'s Comment<'s>> {
    if !node.has_source() {
        return Vec::new();
    }
    let start_line = node.first_line();
    let end_line = find_end_line(ps, node);
    if end_line <= start_line {
        return Vec::new();
    }
    ps.comments_in_lines(start_line, end_line - 1)
}

/// `comments_contain_disables?(node, cop_name)`.
pub fn comments_contain_disables(
    cx: &crate::cop::Investigation<'_, '_>,
    node: Node<'_>,
    cop_name: &str,
) -> bool {
    let Some(ranges) = cx.comment_config().cop_disabled_line_ranges.get(cop_name) else {
        return false;
    };
    let start = node.first_line() as i64;
    let end = find_end_line(cx.processed_source, node) as i64; // exclusive
    ranges
        .iter()
        .any(|r| (r.begin <= start && r.end >= end - 1) || (start <= r.begin && end > r.end))
}

/// `find_end_line(node)`.
pub fn find_end_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let line = |r: Range| ps.line_of(r.begin_pos);
    let found = if node.is(NodeType::If) {
        if node.has_else() {
            node.loc().else_.map(line)
        } else if node.is_ternary() {
            node.else_branch().map(|e| e.first_line())
        } else if node.is_elsif() {
            node.ancestors()
                .filter(|a| a.is(NodeType::If))
                .find(|a| a.is_if())
                .and_then(|a| a.loc().end.map(line))
        } else if node.is_if()
            && let Some(parent) = node.parent()
            && parentheses(parent)
        {
            parent.loc().end.map(line)
        } else {
            None
        }
    } else if node.is_any_block() {
        node.loc().end.map(line)
    } else if let Some(next) = node.right_sibling().filter(|s| s.has_source()) {
        Some(next.first_line())
    } else if let Some(parent) = node.parent() {
        if let Some(end) = parent.loc().end {
            Some(line(end))
        } else {
            Some(parent.first_line())
        }
    } else {
        None
    };
    found.unwrap_or_else(|| node.loc().end.map_or(node.last_line(), line))
}
