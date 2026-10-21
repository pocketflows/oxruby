//! Port of `lib/rubocop/cop/mixin/alignment.rb`.
//!
//! `column_delta` is handed to the cop's `autocorrect` instead of being kept
//! in an instance variable.

use unicode_width::UnicodeWidthStr;

use crate::ast::Node;
use crate::cop::mixin::util::begins_its_line;
use crate::cop::{Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

pub const SPACE: &str = " ";

/// A cop's `message(node)`.
pub type MessageFn<'f, 'a, 's> = dyn Fn(&Investigation<'a, 's>, Option<Node<'a>>) -> String + 'f;

/// A cop's `autocorrect(corrector, node)`, with the `column_delta`.
pub type AutocorrectFn<'f, 'a, 's> =
    dyn Fn(&mut Corrector, &Investigation<'a, 's>, Option<Node<'a>>, isize) + 'f;

/// `configured_indentation_width`: the cop's `IndentationWidth`, else
/// `Layout/IndentationWidth`'s `Width`, else 2.
pub fn configured_indentation_width(cx: &Investigation<'_, '_>) -> usize {
    cx.cop_config_int("IndentationWidth")
        .or_else(|| {
            cx.config
                .for_cop("Layout/IndentationWidth")
                .get("Width")
                .and_then(serde_yaml_ng::Value::as_i64)
        })
        .unwrap_or(2)
        .max(0) as usize
}

/// `indentation(node)`.
pub fn indentation(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    offset(node) + &SPACE.repeat(configured_indentation_width(cx))
}

/// `offset(node)`.
pub fn offset(node: Node<'_>) -> String {
    SPACE.repeat(node.column())
}

/// `check_alignment(items, base_column)`. `message` is the cop's
/// `message(node)` and `autocorrect` its `autocorrect(corrector, node)`;
/// both receive `nil` (`None`) for items inside an earlier offense.
pub fn check_alignment<'a, 's>(
    cx: &mut Investigation<'a, 's>,
    items: &[Node<'a>],
    base_column: Option<usize>,
    message: &MessageFn<'_, 'a, 's>,
    autocorrect: &AutocorrectFn<'_, 'a, 's>,
) {
    let base_column = match base_column {
        Some(column) => column,
        None => match items.first() {
            Some(first) => display_column(cx.processed_source, first.source_range()),
            None => return,
        },
    };
    for (current, column_delta) in each_bad_alignment(cx.processed_source, items, base_column) {
        let expr = current.source_range();
        let inside_offense = cx
            .current_offense_ranges()
            .any(|location| within(expr, location));
        let message_node = (!inside_offense).then_some(current);
        let message = message(cx, message_node);
        cx.add_offense_cx(expr, &message, |corrector, cx| {
            autocorrect(corrector, cx, message_node, column_delta);
        });
    }
}

/// `each_bad_alignment(items, base_column)`: the misaligned items that begin
/// their line, with their `column_delta`.
pub fn each_bad_alignment<'a>(
    ps: &ProcessedSource<'_>,
    items: &[Node<'a>],
    base_column: usize,
) -> Vec<(Node<'a>, isize)> {
    let mut bad = Vec::new();
    let mut prev_line = 0;
    for &current in items {
        if current.first_line() > prev_line && begins_its_line(ps, current.source_range()) {
            let column_delta =
                base_column as isize - display_column(ps, current.source_range()) as isize;
            if column_delta != 0 {
                bad.push((current, column_delta));
            }
        }
        prev_line = current.first_line();
    }
    bad
}

/// `display_column(range)`: the display width of the line up to `range`.
pub fn display_column(ps: &ProcessedSource<'_>, range: Range) -> usize {
    let line = ps.line_of(range.begin_pos);
    let start = ps.line_start(line);
    ps.buffer_source()[start..range.begin_pos].width()
}

/// `within?(inner, outer)`.
pub fn within(inner: Range, outer: Range) -> bool {
    inner.begin_pos >= outer.begin_pos && inner.end_pos <= outer.end_pos
}
