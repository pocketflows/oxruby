//! Port of rubocop-rspec's `lib/rubocop/cop/rspec/mixin/empty_line_separation.rb`.
//!
//! Helps determine the offending location if there is not an empty line
//! following the node. Allows comments to follow directly after
//! in the following cases.
//! - `rubocop:enable` directive
//! - followed by empty line(s)

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::mixin::range_help::source_range;
use crate::directive_comment::DirectiveComment;
use crate::processed_source::Range;

use super::final_end_location::final_end_location;

/// `missing_separating_line_offense(node) { |method| message }`.
pub fn missing_separating_line_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    message: impl FnOnce(&str) -> String,
) {
    if last_child(node) {
        return;
    }

    let Some(location) = missing_separating_line(cx, node) else {
        return;
    };
    let msg = message(node.method_name().unwrap_or_default());
    cx.add_offense_with_correction(location, &msg, |corrector| {
        corrector.insert_after(location.end(), "\n");
    });
}

/// `missing_separating_line(node) { |location| ... }`: the location to
/// report, if any.
pub fn missing_separating_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<Range> {
    let ps = cx.processed_source;
    let final_end_line = node.line_of(final_end_location(node)?);
    let mut line = final_end_line;
    let mut enable_directive_line = None;
    while ps.line_with_comment(line + 1) {
        line += 1;
        let comment = ps.comment_at_line(line)?;
        if DirectiveComment::new(comment.text, comment.range, line as i64).enabled() {
            enable_directive_line = Some(line);
        }
    }

    // `processed_source[line].blank?`: the line after `line` (a missing line
    // is `nil`, which is blank).
    if ps.lines().get(line).is_none_or(|text| blank(text)) {
        return None;
    }

    Some(offending_loc(
        cx,
        enable_directive_line.unwrap_or(final_end_line),
    ))
}

/// `offending_loc(last_line)`: the line's content, without its indentation.
pub fn offending_loc(cx: &Investigation<'_, '_>, last_line: usize) -> Range {
    let ps = cx.processed_source;
    let offending_line = ps.lines().get(last_line - 1).copied().unwrap_or_default();

    let content_length = lstrip(offending_line).chars().count();
    let start = offending_line.chars().count() - content_length;

    source_range(ps, last_line, start, content_length)
}

/// `last_child?(node)`.
pub fn last_child(node: Node<'_>) -> bool {
    let Some(parent) = node.parent().filter(|p| p.is(NodeType::Begin)) else {
        return true;
    };

    parent
        .children()
        .last()
        .and_then(|child| child.as_node())
        .is_some_and(|last| last == node.id())
}

/// Ruby's `String#lstrip`.
fn lstrip(text: &str) -> &str {
    text.trim_start_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r', '\0'])
}

/// RuboCop's `String#blank?`.
fn blank(text: &str) -> bool {
    lstrip(text).is_empty()
}
