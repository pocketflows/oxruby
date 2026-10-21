//! Port of `lib/rubocop/cop/mixin/first_element_line_break.rb`. The cop's
//! `MSG` is passed in (`add_offense(min)` uses the default message).

use crate::ast::Node;
use crate::cop::Investigation;
use crate::cop::correctors::empty_line_corrector;
use crate::regex;

/// `check_method_line_break(node, children, ignore_last:)`.
pub fn check_method_line_break(
    cx: &mut Investigation<'_, '_>,
    msg: &str,
    node: Node<'_>,
    children: &[Node<'_>],
    ignore_last: bool,
) {
    let Some(first) = children.first() else {
        return;
    };
    if !method_uses_parens(cx, node, *first) {
        return;
    }
    check_children_line_break(cx, msg, node, children, None, ignore_last);
}

/// `method_uses_parens?(node, limit)`.
fn method_uses_parens(cx: &Investigation<'_, '_>, node: Node<'_>, limit: Node<'_>) -> bool {
    let line = cx.processed_source.source_line(node.first_line());
    let source: String = line.chars().take(limit.column()).collect();
    regex!(r"\s*\(\s*$").find(&source).is_some()
}

/// `check_children_line_break(node, children, start = node, ignore_last:)`.
pub fn check_children_line_break(
    cx: &mut Investigation<'_, '_>,
    msg: &str,
    node: Node<'_>,
    children: &[Node<'_>],
    start: Option<Node<'_>>,
    ignore_last: bool,
) {
    if children.is_empty() {
        return;
    }
    let line = start.unwrap_or(node).first_line();
    let min = first_by_line(children);
    if line != min.first_line() {
        return;
    }
    let max_line = last_line(children, ignore_last);
    if line == max_line {
        return;
    }
    let range = min.source_range();
    cx.add_offense_with_correction(range, msg, |corrector| {
        empty_line_corrector::insert_before(corrector, range);
    });
}

/// `first_by_line(nodes)`: the first node with the smallest first line.
fn first_by_line<'a>(nodes: &[Node<'a>]) -> Node<'a> {
    let mut min = nodes[0];
    for node in &nodes[1..] {
        if node.first_line() < min.first_line() {
            min = *node;
        }
    }
    min
}

fn last_line(nodes: &[Node<'_>], ignore_last: bool) -> usize {
    nodes
        .iter()
        .map(|n| {
            if ignore_last {
                n.first_line()
            } else {
                n.last_line()
            }
        })
        .max()
        .unwrap_or(0)
}
