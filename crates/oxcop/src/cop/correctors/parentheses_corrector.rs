//! Port of `lib/rubocop/cop/correctors/parentheses_corrector.rb`.

use crate::ast::{MapKind, Node, NodeType};
use crate::cop::Corrector;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::processed_source::Range;
use crate::regex;

pub fn correct(corrector: &mut Corrector, node: Node<'_>) {
    let src = node.ast().source();
    corrector.remove(range_with_surrounding_space(
        src,
        node.loc().begin.unwrap(),
        SurroundingSpace {
            side: Side::Right,
            whitespace: true,
            ..SurroundingSpace::default()
        },
    ));
    remove_close_paren(corrector, node);
    handle_orphaned_comma(corrector, node);
    if ternary_condition(node) && next_char_is_question_mark(node) {
        corrector.insert_after(node.loc().end.unwrap(), " ");
    }
}

fn remove_close_paren(corrector: &mut Corrector, node: Node<'_>) {
    let newlines = !comment_above_close_paren_swallows_chain(node);
    corrector.remove(range_with_surrounding_space(
        node.ast().source(),
        node.loc().end.unwrap(),
        SurroundingSpace {
            side: Side::Left,
            newlines,
            ..SurroundingSpace::default()
        },
    ));
}

fn comment_above_close_paren_swallows_chain(node: Node<'_>) -> bool {
    let Some(last_child) = node.child_nodes().last() else {
        return false;
    };
    let body_end = last_child.source_range().end_pos;
    let close_paren_begin = node.loc().end.unwrap().begin_pos;
    if body_end >= close_paren_begin {
        return false;
    }
    let between = &node.ast().source()[body_end..close_paren_begin];
    if regex!(r"#[^\n]*\n").find(between).is_none() {
        return false;
    }
    chained_after_close_paren(node)
}

fn chained_after_close_paren(node: Node<'_>) -> bool {
    let close_paren = node.loc().end.unwrap();
    let src = node.ast().source();
    let line_end = src[close_paren.end_pos..]
        .find('\n')
        .map_or(src.len(), |i| close_paren.end_pos + i);
    let after = &src[close_paren.end_pos..line_end];
    let trimmed = after.trim_start();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

fn ternary_condition(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::If) && p.is_ternary())
}

fn next_char_is_question_mark(node: Node<'_>) -> bool {
    let question = node.parent().unwrap().loc().question.unwrap();
    node.last_column() == node.column_of(question.begin_pos)
}

fn only_closing_paren_before_comma(node: Node<'_>) -> bool {
    let end = node.loc().end.unwrap();
    let src = node.ast().source();
    let line_start = src[..end.begin_pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[line_start..]
        .find('\n')
        .map_or(src.len(), |i| line_start + i);
    regex!(r"\A\s*\)\s*,")
        .find(&src[line_start..line_end])
        .is_some()
}

fn handle_orphaned_comma(corrector: &mut Corrector, node: Node<'_>) {
    if !only_closing_paren_before_comma(node) {
        return;
    }
    let range = extend_range_for_heredoc(node, parens_range(node));
    corrector.remove(range);
    add_heredoc_comma(corrector, node);
}

fn parens_range(node: Node<'_>) -> Range {
    range_with_surrounding_space(
        node.ast().source(),
        node.loc().end.unwrap(),
        SurroundingSpace {
            side: Side::Left,
            newlines: true,
            whitespace: true,
            continuations: true,
        },
    )
}

fn extend_range_for_heredoc(node: Node<'_>, range: Range) -> Range {
    if !heredoc(node) {
        return range;
    }
    let end = node.loc().end.unwrap();
    let src = node.ast().source();
    let line_start = src[..end.begin_pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[line_start..]
        .find('\n')
        .map_or(src.len(), |i| line_start + i);
    let comma_line = &src[line_start..line_end];
    let offset = regex!(r"(?<=\))\s*,")
        .find(comma_line)
        .map_or(0, |(s, e)| e - s);
    Range::new(range.begin_pos, range.end_pos + offset)
}

fn add_heredoc_comma(corrector: &mut Corrector, node: Node<'_>) {
    if !heredoc(node) {
        return;
    }
    corrector.insert_after(node.child_nodes().last().unwrap().source_range(), ",");
}

fn heredoc(node: Node<'_>) -> bool {
    node.child_nodes()
        .last()
        .is_some_and(|c| c.loc().kind == MapKind::Heredoc)
}
