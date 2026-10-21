//! Port of `lib/rubocop/cop/mixin/percent_array.rb`, for `Style/WordArray`
//! and `Style/SymbolArray`. The cop-specific parts (`ARRAY_MSG`,
//! `PERCENT_MSG`, `build_bracketed_array`, `invalid_percent_array_contents?`)
//! come from the [`PercentArray`] trait.

use crate::ast::Node;
use crate::cop::Investigation;
use crate::cop::correctors::percent_literal_corrector;
use crate::cop::mixin::array_min_size::{
    ArrayStyleState, array_style_detected, below_array_length,
};
use crate::cop::mixin::configurable_enforced_style::{no_acceptable_style_bang, style};

pub trait PercentArray {
    /// `self.class::ARRAY_MSG`, with `%<prefer>s`.
    const ARRAY_MSG: &'static str;
    /// `self.class::PERCENT_MSG`.
    const PERCENT_MSG: &'static str;

    fn build_bracketed_array(&self, cx: &Investigation<'_, '_>, node: Node<'_>) -> String;

    /// The `array_style_detected` bookkeeping.
    fn array_style_state(&mut self) -> &mut ArrayStyleState;

    fn invalid_percent_array_contents(&self, _cx: &Investigation<'_, '_>, _node: Node<'_>) -> bool {
        false
    }
}

/// `invalid_percent_array_context?(node)`.
pub fn invalid_percent_array_context(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is_send()
        && parent.arguments().contains(&node)
        && !parent.is_parenthesized()
        && parent.is_block_literal()
}

/// `allowed_bracket_array?(node)`.
pub fn allowed_bracket_array(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    comments_in_array(cx, node)
        || below_array_length(cx, node)
        || invalid_percent_array_context(node)
}

/// `comments_in_array?(node)`: comments on the lines before the last one.
fn comments_in_array(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    (node.first_line()..node.last_line())
        .any(|line| cx.processed_source.comment_at_line(line).is_some())
}

/// `check_percent_array(node)`.
pub fn check_percent_array<C: PercentArray>(
    cop: &mut C,
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
) {
    let size = node.child_nodes().count() as i64;
    array_style_detected(cx, cop.array_style_state(), "percent", size);
    let brackets_required = cop.invalid_percent_array_contents(cx, node);
    if !(style(cx) == "brackets" || brackets_required) {
        return;
    }
    if brackets_required {
        no_acceptable_style_bang(cx);
    }
    let bracketed_array = cop.build_bracketed_array(cx, node);
    let message = build_message_for_bracketed_array::<C>(&bracketed_array);
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        corrector.replace(node.source_range(), &bracketed_array);
    });
}

fn build_message_for_bracketed_array<C: PercentArray>(preferred_array_code: &str) -> String {
    let prefer = if preferred_array_code.contains('\n') {
        "an array literal `[...]`".to_owned()
    } else {
        format!("`{preferred_array_code}`")
    };
    C::ARRAY_MSG.replace("%<prefer>s", &prefer)
}

/// `check_bracketed_array(node, literal_prefix)`.
pub fn check_bracketed_array<C: PercentArray>(
    cop: &mut C,
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    literal_prefix: &str,
) {
    if allowed_bracket_array(cx, node) {
        return;
    }
    array_style_detected(
        cx,
        cop.array_style_state(),
        "brackets",
        node.child_nodes().count() as i64,
    );
    if style(cx) != "percent" {
        return;
    }
    let config = cx.config;
    cx.add_offense_with_correction(node.source_range(), C::PERCENT_MSG, |corrector| {
        percent_literal_corrector::correct(config, corrector, node, literal_prefix);
    });
}

/// `build_bracketed_array_with_appropriate_whitespace(elements:, node:)`.
pub fn build_bracketed_array_with_appropriate_whitespace(
    elements: &[String],
    node: Node<'_>,
) -> String {
    format!(
        "[{}{}{}]",
        whitespace_leading(node),
        elements.join(&format!(",{}", whitespace_between(node))),
        whitespace_trailing(node)
    )
}

fn whitespace_between(node: Node<'_>) -> &str {
    let children: Vec<Node<'_>> = node.child_nodes().collect();
    if children.len() >= 2 {
        let range = children[0]
            .source_range()
            .end()
            .join(children[1].source_range().begin());
        node.range_source(range)
    } else {
        " "
    }
}

fn whitespace_leading(node: Node<'_>) -> &str {
    let first = node.child_nodes().next().unwrap();
    let range = node
        .loc()
        .begin
        .unwrap()
        .end()
        .join(first.source_range().begin());
    node.range_source(range)
}

fn whitespace_trailing(node: Node<'_>) -> &str {
    let last = node.child_nodes().last().unwrap();
    let range = last
        .source_range()
        .end()
        .join(node.loc().end.unwrap().begin());
    node.range_source(range)
}
