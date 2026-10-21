//! Port of `lib/rubocop/cop/correctors/multiline_literal_brace_corrector.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_literal_brace_layout::{
    closing_brace_on_same_line, new_line_needed_before_closing_brace,
};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_between, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::{Corrector, Investigation};
use crate::processed_source::Range;

/// `MultilineLiteralBraceCorrector.correct(corrector, node, processed_source)`.
/// `children` is `children(node)` (the elements).
pub fn correct(
    corrector: &mut Corrector,
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
) {
    if closing_brace_on_same_line(node, children) {
        corrector.insert_before(node.loc().end.unwrap(), "\n");
    } else {
        if new_line_needed_before_closing_brace(cx, node, children) {
            return;
        }
        let end_range =
            last_element_range_with_trailing_comma(cx.processed_source.buffer_source(), children)
                .end();
        correct_next_line_brace(corrector, cx, node, children, end_range);
        correct_heredoc_argument_method_chain(corrector, node, end_range);
    }
}

fn correct_next_line_brace(
    corrector: &mut Corrector,
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
    end_range: Range,
) {
    let src = cx.processed_source.buffer_source();
    corrector.remove(range_with_surrounding_space(
        src,
        node.loc().end.unwrap(),
        SurroundingSpace::side(Side::Left),
    ));
    let content = content_if_comment_present(corrector, cx, node, children);
    corrector.insert_before(end_range, content);
}

fn correct_heredoc_argument_method_chain(
    corrector: &mut Corrector,
    node: Node<'_>,
    end_range: Range,
) {
    let Some(parent) = node.parent() else {
        return;
    };
    if !use_heredoc_argument_method_chain(node, parent) {
        return;
    }
    let chained_method = range_between(
        parent.loc().dot.unwrap().begin_pos,
        parent.source_range().end_pos,
    );
    corrector.remove(chained_method);
    corrector.insert_after(end_range, parent.range_source(chained_method));
}

fn content_if_comment_present(
    corrector: &mut Corrector,
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
) -> String {
    let ps = cx.processed_source;
    if ps
        .comment_at_line(children.last().unwrap().last_line())
        .is_some()
    {
        select_content_to_be_inserted_after_last_element(corrector, cx, node)
    } else {
        node.range_source(node.loc().end.unwrap()).to_owned()
    }
}

fn use_heredoc_argument_method_chain(node: Node<'_>, parent: Node<'_>) -> bool {
    // `node.respond_to?(:first_argument)`: send-like nodes.
    if !matches!(
        node.ty(),
        NodeType::Send
            | NodeType::Csend
            | NodeType::Super
            | NodeType::Zsuper
            | NodeType::Yield
            | NodeType::Defined
            | NodeType::Index
            | NodeType::Indexasgn
            | NodeType::Lambda
            | NodeType::Def
            | NodeType::Defs
            | NodeType::Block
            | NodeType::Return
            | NodeType::Break
            | NodeType::Next
    ) {
        return false;
    }
    let Some(first_argument) = node.first_argument() else {
        return false;
    };
    parent.is_call() && first_argument.is(NodeType::Str) && first_argument.is_heredoc()
}

fn select_content_to_be_inserted_after_last_element(
    corrector: &mut Corrector,
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
) -> String {
    let range = range_between(
        node.loc().end.unwrap().begin_pos,
        range_by_whole_lines(cx.processed_source, node.source_range(), false).end_pos,
    );
    corrector.remove(range);
    node.range_source(range).to_owned()
}

/// `last_element_range_with_trailing_comma(node)`.
pub fn last_element_range_with_trailing_comma(src: &str, children: &[Node<'_>]) -> Range {
    let last = children.last().unwrap().source_range();
    match last_element_trailing_comma_range(src, last) {
        Some(comma) => last.join(comma),
        None => last,
    }
}

fn last_element_trailing_comma_range(src: &str, last: Range) -> Option<Range> {
    let range = range_with_surrounding_space(src, last, SurroundingSpace::side(Side::Right)).end();
    let range = range.resize(1);
    (src.get(range.begin_pos..range.end_pos) == Some(",")).then_some(range)
}
