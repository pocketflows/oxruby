//! Port of `lib/rubocop/cop/mixin/statement_modifier.rb`: shared logic of
//! the cops that check for (or against) modifier `if`/`unless`/`while`/
//! `until`.
//!
//! Cops that override `non_eligible_node?` (calling `super`) pass their own
//! check to [`single_line_as_modifier_with`]; the default is
//! [`non_eligible_node`].

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::mixin::line_length_help::{
    ExcessiveType, allow_cop_directives, allow_uri, allowed_position, directive_on_source_line,
    find_excessive_range, line_length, line_length_without_directive, max_line_length,
};
use crate::cop::mixin::ruby_regex;
use crate::processed_source::Range;

/// `single_line_as_modifier?(node)`.
pub fn single_line_as_modifier(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    single_line_as_modifier_with(cx, node, non_eligible_node)
}

/// `single_line_as_modifier?` with the cop's own `non_eligible_node?`.
pub fn single_line_as_modifier_with<'a>(
    cx: &Investigation<'_, '_>,
    node: Node<'a>,
    non_eligible_node: impl Fn(&Investigation<'_, '_>, Node<'a>) -> bool,
) -> bool {
    if non_eligible_node(cx, node)
        || non_eligible_body(cx, node.body())
        || node.condition().is_none_or(non_eligible_condition)
    {
        return false;
    }
    modifier_fits_on_single_line(cx, node)
}

/// `nonempty_line_count`: the lines of the node's source that are not blank.
fn nonempty_line_count(node: Node<'_>) -> usize {
    node.source()
        .split_inclusive('\n')
        .filter(|l| l.chars().any(|c| !c.is_whitespace()))
        .count()
}

/// `non_eligible_node?(node)` (the mixin's version).
pub fn non_eligible_node(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_modifier_form()
        || nonempty_line_count(node) > 3
        || cx.processed_source.line_with_comment(node.last_line())
        || (first_line_comment(cx, node).is_some() && code_after(cx, node).is_some())
}

pub fn non_eligible_body(cx: &Investigation<'_, '_>, body: Option<Node<'_>>) -> bool {
    match body {
        None => true,
        Some(body) => {
            body.is_empty_source()
                || body.is(NodeType::Begin)
                || cx.processed_source.contains_comment(body.source_range())
        }
    }
}

pub fn non_eligible_condition(condition: Node<'_>) -> bool {
    condition.each_node().any(|n| n.is(NodeType::Lvasgn))
}

/// `modifier_fits_on_single_line?`: the rendered modifier form is compared
/// against the bare maximum, without `Layout/LineLength`'s exemptions.
pub fn modifier_fits_on_single_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(max) = max_line_length(cx) else {
        return true;
    };
    line_length(cx, &line_in_modifier_form(cx, node)) <= max
}

pub fn line_in_modifier_form(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let keyword = node.loc().keyword.unwrap();
    let ps = cx.processed_source;
    let line = ps.source_line(ps.line_of(keyword.begin_pos));
    let column = ps.column_of(keyword.begin_pos);
    let code_before: String = line.chars().take(column).collect();
    format!(
        "{code_before}{}{}",
        to_modifier_form(cx, node),
        code_after(cx, node).unwrap_or_default()
    )
}

/// `acceptable_line_length?(line, line_number)`: whether an existing line is
/// acceptable to `Layout/LineLength`, its exemptions included.
pub fn acceptable_line_length(cx: &Investigation<'_, '_>, line: &str, line_number: usize) -> bool {
    let Some(max) = max_line_length(cx) else {
        return true;
    };
    if line_length(cx, line) <= max {
        return true;
    }
    if !line_length_enabled_at_line(cx, line_number) {
        return true;
    }
    if matches_allowed_pattern(cx, line) {
        return true;
    }
    if allow_cop_directives(cx) && directive_on_source_line(cx, line_number - 1) {
        return line_length_without_directive(line) <= max;
    }
    allowed_by_uri(cx, line)
}

pub fn allowed_by_uri(cx: &Investigation<'_, '_>, line: &str) -> bool {
    if !allow_uri(cx) {
        return false;
    }
    find_excessive_range(cx, line, ExcessiveType::Uri)
        .is_some_and(|range| allowed_position(cx, line, &range))
}

fn line_length_enabled_at_line(cx: &Investigation<'_, '_>, line: usize) -> bool {
    cx.comment_config()
        .cop_enabled_at_lines("Layout/LineLength", line as i64, line as i64)
}

/// `Layout/LineLength`'s allowed patterns (`allowed_patterns` overridden).
fn matches_allowed_pattern(cx: &Investigation<'_, '_>, line: &str) -> bool {
    let config = cx.config.for_cop("Layout/LineLength");
    let patterns = config
        .get("AllowedPatterns")
        .filter(|v| !v.is_null())
        .or_else(|| config.get("IgnoredPatterns").filter(|v| !v.is_null()));
    let Some(serde_yaml_ng::Value::Sequence(patterns)) = patterns else {
        return false;
    };
    patterns
        .iter()
        .filter_map(serde_yaml_ng::Value::as_str)
        .filter_map(ruby_regex)
        .any(|re| re.find(line).is_some())
}

pub fn to_modifier_form(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let body = node.body().map(if_body_source);
    let expression = [
        body,
        Some(node.keyword().to_owned()),
        node.condition().map(|c| c.source().to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    let parenthesized = if parenthesize(node) {
        format!("({expression})")
    } else {
        expression
    };
    match first_line_comment(cx, node) {
        Some(comment) => format!("{parenthesized} {comment}"),
        None => parenthesized,
    }
}

fn if_body_source(if_body: Node<'_>) -> String {
    if if_body.is_call() && !if_body.is_method("[]=") && omitted_value_in_last_hash_arg(if_body) {
        let args: Vec<&str> = if_body.arguments().iter().map(|a| a.source()).collect();
        format!("{}({})", method_source(if_body), args.join(", "))
    } else {
        if_body.source().to_owned()
    }
}

fn omitted_value_in_last_hash_arg(if_body: Node<'_>) -> bool {
    let Some(last_argument) = if_body.last_argument() else {
        return false;
    };
    last_argument.is(NodeType::Hash)
        && last_argument
            .pairs()
            .last()
            .is_some_and(|p| p.is_value_omission())
}

fn method_source(if_body: Node<'_>) -> &str {
    let end_range = if if_body.is_implicit_call() {
        if_body.loc().dot.unwrap().end()
    } else {
        if_body.loc().selector.unwrap()
    };
    if_body.range_source(if_body.source_range().begin().join(end_range))
}

pub fn first_line_comment<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> Option<&'s str> {
    let comment = cx.processed_source.comment_at_line(node.first_line())?;
    let text = comment.text;
    (!comment_disables_cop(cx, text)).then_some(text)
}

pub fn code_after<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> Option<&'s str> {
    let end: Range = node.loc().end?;
    let ps = cx.processed_source;
    let line = ps.source_line(ps.line_of(end.end_pos));
    let last_column = ps.column_of(end.end_pos);
    let byte = line
        .char_indices()
        .nth(last_column)
        .map_or(line.len(), |(i, _)| i);
    let code = &line[byte..];
    (!code.is_empty()).then_some(code)
}

/// `parenthesize?(node)`: changing to modifier form would change the
/// meaning of the parent expression (modifier `if` has low precedence).
pub fn parenthesize(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let assignment = if parent.is_call() {
        parent.is_setter_method()
    } else {
        parent.is_assignment()
    };
    if assignment || parent.is_operator_keyword() {
        return true;
    }
    if parent.is_any(&[NodeType::Array, NodeType::Pair]) {
        return true;
    }
    parent.is(NodeType::Send)
}

fn comment_disables_cop(cx: &Investigation<'_, '_>, comment: &str) -> bool {
    let pattern = format!(
        "# rubocop : (disable|todo) ([^,],)* (all|{})",
        cx.cop_name()
    );
    ruby_regex(&pattern.replace(' ', r"\s*")).is_some_and(|re| re.find(comment).is_some())
}
