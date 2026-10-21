//! Port of `lib/rubocop/cop/mixin/trailing_comma.rb`, shared by
//! `Style/TrailingCommaInArguments`, `...InArrayLiteral` and
//! `...InHashLiteral`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::Investigation;
use crate::cop::correctors::punctuation_corrector::swap_comma;
use crate::cop::mixin::configurable_enforced_style::style_for;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::begins_its_line;
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

/// `style_parameter_name`.
pub const STYLE_PARAMETER_NAME: &str = "EnforcedStyleForMultiline";

fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, STYLE_PARAMETER_NAME)
}

/// `check(node, items, kind, begin_pos, end_pos)`. `kind` contains
/// `%<article>s`.
pub fn check(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    items: &[Node<'_>],
    kind: &str,
    begin_pos: usize,
    end_pos: usize,
) {
    let after_last_item = range_between(begin_pos, end_pos);
    let comma_offset = comma_offset(cx, items, after_last_item);
    let style = style(cx);
    match comma_offset {
        Some(offset) if !inside_comment(cx.processed_source, after_last_item, offset) => {
            check_comma(cx, node, kind, after_last_item.begin_pos + offset);
        }
        _ => {
            if should_have_comma(cx.processed_source, style, node) {
                put_comma(cx, items, kind);
            }
        }
    }
}

fn comma_offset(cx: &Investigation<'_, '_>, items: &[Node<'_>], range: Range) -> Option<usize> {
    let source = cx.source(range);
    let matches = if any_heredoc(items) {
        regex!(r"\A[^\S\n]*,").find(source).is_some()
    } else {
        regex!(r"\A\s*,").find(source).is_some()
    };
    if matches { source.find(',') } else { None }
}

fn check_comma(cx: &mut Investigation<'_, '_>, node: Node<'_>, kind: &str, comma_pos: usize) {
    let style = style(cx);
    if should_have_comma(cx.processed_source, style, node) {
        return;
    }
    avoid_comma(cx, kind, comma_pos, extra_avoid_comma_info(style));
}

/// `check_literal(node, kind)`.
pub fn check_literal(cx: &mut Investigation<'_, '_>, node: Node<'_>, kind: &str) {
    let items: Vec<Node<'_>> = node.child_nodes().collect();
    let Some(last) = items.last() else {
        return;
    };
    let Some(end) = node.loc().end else {
        return;
    };
    check(
        cx,
        node,
        &items,
        kind,
        last.source_range().end_pos,
        end.begin_pos,
    );
}

fn extra_avoid_comma_info(style: &str) -> &'static str {
    match style {
        "comma" => ", unless each item is on its own line",
        "consistent_comma" => ", unless items are split onto multiple lines",
        "diff_comma" => ", unless that item immediately precedes a newline",
        _ => "",
    }
}

fn should_have_comma(ps: &ProcessedSource<'_>, style: &str, node: Node<'_>) -> bool {
    match style {
        "comma" => multiline(ps, node) && no_elements_on_same_line(ps, node),
        "consistent_comma" => multiline(ps, node) && !method_name_and_arguments_on_same_line(node),
        "diff_comma" => multiline(ps, node) && last_item_precedes_newline(node),
        _ => false,
    }
}

fn inside_comment(ps: &ProcessedSource<'_>, range: Range, comma_offset: usize) -> bool {
    ps.comment_at_line(ps.line_of(range.begin_pos))
        .is_some_and(|comment| comment.range.begin_pos < range.begin_pos + comma_offset)
}

fn multiline(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    node.is_multiline() && !allowed_multiline_argument(ps, node)
}

fn method_name_and_arguments_on_same_line(node: Node<'_>) -> bool {
    if !node.is_call() {
        return false;
    }
    let Some(last_argument) = node.last_argument() else {
        return false;
    };
    if node.last_line() != last_argument.last_line() {
        return false;
    }
    if last_argument.is(NodeType::Hash) && last_argument.has_braces() {
        return true;
    }
    let line = match node.loc().selector {
        Some(selector) => node.line_of(selector),
        None => node.first_line(),
    };
    line == last_argument.last_line()
}

fn allowed_multiline_argument(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    elements(node).len() == 1 && !begins_its_line(ps, node_end_location(node))
}

fn elements(node: Node<'_>) -> Vec<Node<'_>> {
    if !node.is_call() {
        return node.child_nodes().collect();
    }
    node.arguments()
        .into_iter()
        .flat_map(|argument| {
            if argument.is(NodeType::Hash) && argument.is_multiline() && !argument.has_braces() {
                argument.child_nodes().collect()
            } else {
                vec![argument]
            }
        })
        .collect()
}

fn no_elements_on_same_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let mut items: Vec<Range> = elements(node).iter().map(|e| e.source_range()).collect();
    items.push(node_end_location(node));
    !items
        .windows(2)
        .any(|pair| ps.line_of(pair[0].end_pos) == ps.line_of(pair[1].begin_pos))
}

fn node_end_location(node: Node<'_>) -> Range {
    node.loc().end.unwrap_or_else(|| {
        let end = node.source_range().end();
        Range::new(end.begin_pos.saturating_sub(1), end.end_pos)
    })
}

fn last_item_precedes_newline(node: Node<'_>) -> bool {
    let Some(Child::Node(last)) = node.children().last() else {
        return false;
    };
    let last = node.ast().node(*last);
    let after_last_item = last.source_range().end().join(node.source_range().end());
    regex!(r"\A,?\s*(#.*)?\n")
        .find(node.range_source(after_last_item))
        .is_some()
}

fn avoid_comma(
    cx: &mut Investigation<'_, '_>,
    kind: &str,
    comma_begin_pos: usize,
    extra_info: &str,
) {
    let range = range_between(comma_begin_pos, comma_begin_pos + 1);
    let article = if kind.contains("array") { "an" } else { "a" };
    let message = format!(
        "Avoid comma after the last {}{extra_info}.",
        kind.replace("%<article>s", article)
    );
    let source = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(range, &message, |corrector| {
        swap_comma(corrector, Some(range), source);
    });
}

fn put_comma(cx: &mut Investigation<'_, '_>, items: &[Node<'_>], kind: &str) {
    let Some(last_item) = items.last() else {
        return;
    };
    if last_item.is(NodeType::BlockPass) {
        return;
    }
    let range = autocorrect_range(*last_item);
    let message = format!(
        "Put a comma after the last {}.",
        kind.replace("%<article>s", "a multiline")
    );
    let source = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(range, &message, |corrector| {
        swap_comma(corrector, Some(range), source);
    });
}

fn autocorrect_range(item: Node<'_>) -> Range {
    let expr = item.source_range();
    let source = item.source();
    let mut ix = source.rfind('\n').unwrap_or(0);
    ix += source[ix..].find(|c: char| !c.is_whitespace()).unwrap_or(0);
    range_between(expr.begin_pos + ix, expr.end_pos)
}

fn any_heredoc(items: &[Node<'_>]) -> bool {
    items.iter().any(|item| heredoc(Some(*item)))
}

fn heredoc(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if node.loc().heredoc_body.is_some() {
        return true;
    }
    if node.is_send() {
        return heredoc_send(node);
    }
    if node.is_any(&[NodeType::Pair, NodeType::Hash]) {
        return last_child_heredoc(node);
    }
    false
}

fn last_child_heredoc(node: Node<'_>) -> bool {
    match node.children().last() {
        Some(Child::Node(id)) => heredoc(Some(node.ast().node(*id))),
        _ => false,
    }
}

fn heredoc_send(node: Node<'_>) -> bool {
    let size = node.children().len();
    if size == 2 {
        return heredoc(node.child(0));
    }
    if size > 2 {
        return last_child_heredoc(node);
    }
    false
}
