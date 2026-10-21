//! Port of `lib/rubocop/cop/layout/first_argument_indentation.rb`.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{check_alignment, configured_indentation_width, display_column};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::{begins_its_line, comment_line};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

static ELIGIBLE_METHOD_CALL: NodePattern = NodePattern::new("(send _ !:[]= ...)");

#[derive(Default)]
pub struct FirstArgumentIndentation;

impl Cop for FirstArgumentIndentation {
    fn name(&self) -> &'static str {
        "Layout/FirstArgumentIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Super]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !should_check(node) {
            return;
        }
        let first_argument = node.first_argument().unwrap();
        if node.first_line() == first_argument.first_line() {
            return;
        }
        if enforce_first_argument_with_fixed_indentation(cx)
            && (!cx.config.cop_enabled_in_config("Layout/FirstMethodArgumentLineBreak")
                || conflicts_with_fixed_indentation_alignment(cx, node))
        {
            return;
        }
        let indent = base_indentation(cx, node) + configured_indentation_width(cx);
        check_alignment(cx, &[first_argument], Some(indent), &message, &autocorrect);
    }
}

fn should_check(node: Node<'_>) -> bool {
    node.has_arguments() && !bare_operator(node) && !node.is_setter_method()
}

fn autocorrect<'a>(corrector: &mut Corrector, cx: &Investigation<'a, '_>, node: Option<Node<'a>>, column_delta: isize) {
    let Some(node) = node else {
        return;
    };
    let Some(send_node) = node.parent() else {
        return;
    };
    let top_level_send = find_top_level_send(send_node);
    let node_to_correct = if should_correct_entire_chain(cx, send_node, top_level_send, column_delta) {
        top_level_send
    } else {
        node
    };
    alignment_corrector::correct(
        corrector,
        cx.processed_source,
        cx.config,
        Some(Target::Node(node_to_correct)),
        column_delta,
        false,
    );
}

fn should_correct_entire_chain(
    cx: &Investigation<'_, '_>,
    send_node: Node<'_>,
    top_level_send: Node<'_>,
    column_delta: isize,
) -> bool {
    if style(cx) != "special_for_inner_method_call_in_parentheses" {
        return false;
    }
    if !inner_call(top_level_send) {
        return false;
    }
    if display_column(cx.processed_source, send_node.source_range()) as isize >= column_delta.abs() {
        return false;
    }
    top_level_send != send_node
        || top_level_send
            .loc()
            .end
            .is_some_and(|end| begins_its_line(cx.processed_source, end))
}

fn inner_call(top_level_send: Node<'_>) -> bool {
    top_level_send
        .parent()
        .is_some_and(|outer| outer.is_send() && outer.is_parenthesized())
}

fn find_top_level_send(send_node: Node<'_>) -> Node<'_> {
    let mut top_level_send = send_node;
    while let Some(parent) = top_level_send.parent() {
        if !(parent.is_send()
            && parent.receiver().is_some_and(|r| r.same_as(top_level_send))
            && parent.loc().dot.is_some())
        {
            break;
        }
        top_level_send = parent;
    }
    top_level_send
}

fn bare_operator(node: Node<'_>) -> bool {
    node.is_operator_method() && !node.is_dot()
}

fn message(cx: &Investigation<'_, '_>, arg_node: Option<Node<'_>>) -> String {
    let Some(arg_node) = arg_node else {
        return "Bad indentation of the first argument.".to_owned();
    };
    let send_node = arg_node.parent().unwrap();
    let text = cx.processed_source.slice(base_range(send_node, arg_node)).trim();
    let base = if !text.contains('\n') && special_inner_call_indentation(cx, send_node) {
        format!("`{text}`")
    } else if text.lines().next_back().is_some_and(comment_line) {
        "the start of the previous line (not counting the comment)".to_owned()
    } else {
        "the start of the previous line".to_owned()
    };
    format!("Indent the first argument one step more than {base}.")
}

fn base_indentation(cx: &Investigation<'_, '_>, node: Node<'_>) -> usize {
    let first_argument = node.first_argument().unwrap();
    if special_inner_call_indentation(cx, node) {
        column_of(cx.processed_source, base_range(node, first_argument))
    } else {
        first_non_blank(previous_code_line(cx.processed_source, first_argument.first_line()))
    }
}

fn special_inner_call_indentation(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let style = style(cx);
    if style == "consistent" {
        return false;
    }
    if style == "consistent_relative_to_receiver" {
        return true;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if !ELIGIBLE_METHOD_CALL.is_match(parent) {
        return false;
    }
    if !parent.is_parenthesized() && style == "special_for_inner_method_call_in_parentheses" {
        return false;
    }
    node.source_range().begin_pos > parent.source_range().begin_pos
}

fn base_range(send_node: Node<'_>, arg_node: Node<'_>) -> Range {
    let start_node = match send_node.parent() {
        Some(parent) if parent.is_any(&[NodeType::Splat, NodeType::Kwsplat]) => parent,
        _ => send_node,
    };
    range_between(start_node.source_range().begin_pos, arg_node.source_range().begin_pos)
}

fn column_of(ps: &ProcessedSource<'_>, range: Range) -> usize {
    let source = ps.slice(range).trim();
    if source.contains('\n') {
        let line = ps.line_of(range.begin_pos) + source.matches('\n').count() + 1;
        first_non_blank(previous_code_line(ps, line))
    } else {
        display_column(ps, range)
    }
}

fn first_non_blank(line: &str) -> usize {
    line.chars().position(|c| !c.is_whitespace()).unwrap_or(0)
}

fn previous_code_line<'s>(ps: &ProcessedSource<'s>, mut line_number: usize) -> &'s str {
    let comment_lines: Vec<usize> = ps
        .comments()
        .iter()
        .filter(|c| begins_its_line(ps, c.range))
        .map(|c| ps.line_of(c.range.begin_pos))
        .collect();
    let mut line = "";
    while line.chars().all(char::is_whitespace) || comment_lines.contains(&line_number) {
        if line_number <= 1 {
            return line;
        }
        line_number -= 1;
        line = ps.lines()[line_number - 1];
    }
    line
}

fn enforce_first_argument_with_fixed_indentation(cx: &Investigation<'_, '_>) -> bool {
    cx.config
        .for_enabled_cop("Layout/ArgumentAlignment")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("with_fixed_indentation")
}

fn conflicts_with_fixed_indentation_alignment(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !special_inner_call_indentation(cx, node) || !argument_alignment_applies(node) {
        return false;
    }
    base_indentation(cx, node) != indentation_of_method_line(cx.processed_source, node)
}

fn argument_alignment_applies(node: Node<'_>) -> bool {
    if !node.is_call() || node.is_method("[]=") {
        return false;
    }
    node.arguments().len() >= 2
        || node
            .first_argument()
            .is_some_and(|first| first.is(NodeType::Hash) && first.pairs().len() >= 2)
}

fn indentation_of_method_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let range = node.loc().selector.or(node.loc().begin).unwrap();
    first_non_blank(ps.lines()[ps.line_of(range.begin_pos) - 1])
}
