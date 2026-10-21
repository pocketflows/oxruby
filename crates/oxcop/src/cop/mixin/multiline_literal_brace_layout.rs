//! Port of `lib/rubocop/cop/mixin/multiline_literal_brace_layout.rb`.
//!
//! A cop passes the literal and its elements (`children(node)`, which
//! `MultilineMethodCallBraceLayout` overrides to the arguments) together
//! with its four messages.

use crate::ast::{Child, Node};
use crate::cop::Investigation;
use crate::cop::correctors::multiline_literal_brace_corrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::same_line;

/// The cop's `SAME_LINE_MESSAGE`, `NEW_LINE_MESSAGE`,
/// `ALWAYS_NEW_LINE_MESSAGE` and `ALWAYS_SAME_LINE_MESSAGE`.
pub struct Messages {
    pub same_line: &'static str,
    pub new_line: &'static str,
    pub always_new_line: &'static str,
    pub always_same_line: &'static str,
}

/// `check_brace_layout(node)`; `children` is `children(node)`.
pub fn check_brace_layout<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    children: &[Node<'a>],
    messages: &Messages,
) {
    if ignored_literal(node, children) {
        return;
    }
    if let Some(Child::Node(id)) = node.children().last() {
        let last = node.ast().node(*id);
        if last_line_heredoc(last, last) {
            return;
        }
    }
    check(cx, node, children, messages);
}

/// `new_line_needed_before_closing_brace?(node)`.
pub fn new_line_needed_before_closing_brace(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
) -> bool {
    let ps = cx.processed_source;
    let last_element_line = ps.line_of(
        multiline_literal_brace_corrector::last_element_range_with_trailing_comma(
            ps.buffer_source(),
            children,
        )
        .end_pos,
    );
    ps.comment_at_line(last_element_line).is_some() && (node.is_chained() || node.is_argument())
}

fn check<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    children: &[Node<'a>],
    messages: &Messages,
) {
    match style(cx) {
        "symmetrical" => check_symmetrical(cx, node, children, messages),
        "new_line" if closing_brace_on_same_line(node, children) => {
            register(cx, node, children, messages.always_new_line);
        }
        "same_line" if !closing_brace_on_same_line(node, children) => {
            register(cx, node, children, messages.always_same_line);
        }
        _ => {}
    }
}

fn check_symmetrical<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    children: &[Node<'a>],
    messages: &Messages,
) {
    if opening_brace_on_same_line(cx, node, children) {
        if !closing_brace_on_same_line(node, children) {
            register(cx, node, children, messages.same_line);
        }
    } else if closing_brace_on_same_line(node, children) {
        register(cx, node, children, messages.new_line);
    }
}

fn register<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    children: &[Node<'a>],
    message: &str,
) {
    let children = children.to_vec();
    cx.add_offense_cx(node.loc().end.unwrap(), message, move |corrector, cx| {
        multiline_literal_brace_corrector::correct(corrector, cx, node, &children);
    });
}

/// `ignored_literal?(node)`: implicit, empty or single-line.
pub fn ignored_literal(node: Node<'_>, children: &[Node<'_>]) -> bool {
    node.loc().begin.is_none() || children.is_empty() || node.is_single_line()
}

fn opening_brace_on_same_line(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
) -> bool {
    same_line(
        cx.processed_source,
        node.loc().begin.unwrap(),
        children[0].source_range(),
    )
}

/// `closing_brace_on_same_line?(node)`.
pub fn closing_brace_on_same_line(node: Node<'_>, children: &[Node<'_>]) -> bool {
    node.line_of(node.loc().end.unwrap()) == children.last().unwrap().last_line()
}

/// `last_line_heredoc?(node, parent)`.
fn last_line_heredoc(node: Node<'_>, parent: Node<'_>) -> bool {
    if let Some(heredoc_end) = node.loc().heredoc_end
        && node.last_line_of(heredoc_end) >= parent.last_line()
    {
        return true;
    }
    node.child_nodes()
        .any(|child| last_line_heredoc(child, parent))
}
