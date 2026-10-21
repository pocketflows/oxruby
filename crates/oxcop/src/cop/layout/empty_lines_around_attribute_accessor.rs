//! Port of `lib/rubocop/cop/layout/empty_lines_around_attribute_accessor.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::ProcessedSource;

const MSG: &str = "Add an empty line after attribute accessor.";

#[derive(Default)]
pub struct EmptyLinesAroundAttributeAccessor;

impl Cop for EmptyLinesAroundAttributeAccessor {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundAttributeAccessor"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["attr_reader", "attr_writer", "attr_accessor", "attr"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.attribute_accessor().is_none() {
            return;
        }
        let ps = cx.processed_source;
        if next_line_empty(ps, node.last_line())
            || next_line_empty_or_enable_directive_comment(ps, node.last_line())
        {
            return;
        }
        if !require_empty_line(cx, next_line_node(node)) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let mut node_range = range_by_whole_lines(ps, node.source_range(), false);
            let next_line = ps.line_of(node_range.end_pos) + 1;
            if next_line_enable_directive_comment(ps, next_line) {
                node_range = ps.comment_at_line(next_line).unwrap().range;
            }
            corrector.insert_after(node_range, "\n");
        });
    }
}

fn next_line_empty_or_enable_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    if next_line_empty(ps, line) {
        return true;
    }
    let next_line = line + 1;
    next_line_enable_directive_comment(ps, next_line) && next_line_empty(ps, next_line)
}

fn next_line_enable_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    let Some(comment) = ps.comment_at_line(line) else {
        return false;
    };
    DirectiveComment::new(comment.text, comment.range, line as i64).enabled()
}

fn next_line_empty(ps: &ProcessedSource<'_>, line: usize) -> bool {
    ps.lines()
        .get(line)
        .is_none_or(|l| l.chars().all(char::is_whitespace))
}

fn require_empty_line(cx: &Investigation<'_, '_>, node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    !allow_alias(cx, node) && !attribute_or_allowed_method(cx, node)
}

fn next_line_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.parent()?.is(NodeType::If) {
        return None;
    }
    node.right_sibling()
}

fn allow_alias(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowAliasSyntax", true) && node.is(NodeType::Alias)
}

fn attribute_or_allowed_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !node.is_send() {
        return false;
    }
    node.attribute_accessor().is_some()
        || node.method_name().is_some_and(|name| allowed_method(cx, name))
}
