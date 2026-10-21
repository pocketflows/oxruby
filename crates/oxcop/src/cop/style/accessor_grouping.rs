//! Port of `lib/rubocop/cop/style/accessor_grouping.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::mixin::util::comment_line;
use crate::cop::mixin::visibility_help::node_visibility;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct AccessorGrouping;

impl Cop for AccessorGrouping {
    fn name(&self) -> &'static str {
        "Style/AccessorGrouping"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let grouped = style(cx) == "grouped";
        for send_node in class_send_elements(node) {
            if send_node.attribute_accessor().is_none() {
                continue;
            }
            check(cx, send_node, grouped);
        }
    }
}

fn check(cx: &mut Investigation<'_, '_>, send_node: Node<'_>, grouped: bool) {
    let ps = cx.processed_source;
    if previous_line_comment(ps, send_node) || !groupable_accessor(ps, send_node) {
        return;
    }
    let offense = if grouped {
        groupable_sibling_accessors(ps, send_node).len() > 1
    } else {
        style(cx) == "separated" && send_node.arguments().len() > 1
    };
    if !offense {
        return;
    }
    let method_name = send_node.method_name().unwrap_or("");
    let message = if grouped {
        format!("Group together all `{method_name}` attributes.")
    } else {
        format!("Use one attribute per `{method_name}`.")
    };
    cx.add_offense_with_correction(send_node.source_range(), &message, |corrector| {
        if let Some(preferred) = preferred_accessors(ps, send_node, grouped) {
            let range = if grouped {
                send_node.source_range()
            } else {
                range_with_trailing_argument_comment(ps, send_node)
            };
            corrector.replace(range, preferred);
        } else {
            let range = range_with_surrounding_space(
                ps.buffer_source(),
                send_node.source_range(),
                SurroundingSpace::side(Side::Left),
            );
            corrector.remove(range);
        }
    });
}

fn previous_line_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    // `processed_source[node.first_line - 2]`: index -1 is the last line.
    let lines = ps.lines();
    let line = match node.first_line().checked_sub(2) {
        Some(index) => lines.get(index),
        None => lines.last(),
    };
    line.is_some_and(|l| comment_line(l))
}

fn groupable_accessor(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let Some(mut previous_expression) = node.left_siblings().last().copied() else {
        return true;
    };
    if previous_expression.is(NodeType::Block)
        && let Some(send) = previous_expression.child_nodes().find(|c| c.is_send())
    {
        previous_expression = send;
    }
    if !previous_expression.is_send() {
        return true;
    }
    let previous_line = previous_expression.first_line();
    if ps
        .comments()
        .iter()
        .any(|c| ps.line_of(c.range.begin_pos) == previous_line && c.text.starts_with("#:"))
    {
        return false;
    }
    previous_expression.attribute_accessor().is_some()
        || previous_expression.is_access_modifier()
        || node.first_line() as isize - previous_expression.last_line() as isize > 1
}

fn class_send_elements(class_node: Node<'_>) -> Vec<Node<'_>> {
    match class_node.body() {
        None => Vec::new(),
        Some(body) if body.is(NodeType::Def) => Vec::new(),
        Some(body) if body.is_send() => vec![body],
        Some(body) => body.child_nodes().filter(|c| c.is_send()).collect(),
    }
}

fn groupable_sibling_accessor(ps: &ProcessedSource<'_>, node: Node<'_>, sibling: Node<'_>) -> bool {
    sibling.attribute_accessor().is_some()
        && sibling.method_name() == node.method_name()
        && node_visibility(sibling) == node_visibility(node)
        && groupable_accessor(ps, sibling)
        && !previous_line_comment(ps, sibling)
}

fn groupable_sibling_accessors<'a>(ps: &ProcessedSource<'_>, send_node: Node<'a>) -> Vec<Node<'a>> {
    send_node
        .parent()
        .unwrap()
        .child_nodes()
        .filter(|sibling| sibling.is_send() && groupable_sibling_accessor(ps, send_node, *sibling))
        .collect()
}

fn preferred_accessors(ps: &ProcessedSource<'_>, node: Node<'_>, grouped: bool) -> Option<String> {
    if grouped {
        if skip_for_grouping(ps, node) {
            return None;
        }
        let accessors = groupable_sibling_accessors(ps, node);
        let first = *accessors.first()?;
        if node.source_range() == first.source_range() || skip_for_grouping(ps, first) {
            Some(group_accessors(node, &accessors))
        } else {
            None
        }
    } else {
        Some(separate_accessors(ps, node))
    }
}

fn skip_for_grouping(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let right = node.right_siblings();
    right.iter().any(|n| n.is(NodeType::Casgn))
        && right
            .iter()
            .any(|n| n.is_send() && groupable_sibling_accessor(ps, node, *n))
}

fn group_accessors(node: Node<'_>, accessors: &[Node<'_>]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for accessor in accessors {
        for arg in accessor.arguments() {
            if !names.contains(&arg.source()) {
                names.push(arg.source());
            }
        }
    }
    format!("{} {}", node.method_name().unwrap_or(""), names.join(", "))
}

fn separate_accessors(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    let method_name = node.method_name().unwrap_or("");
    let first = node.first_argument();
    let indent = " ".repeat(node.column());
    let mut out: Vec<String> = Vec::new();
    for arg in node.arguments() {
        let mut lines: Vec<String> = ps
            .node_comments(arg)
            .iter()
            .map(|c| c.text.to_owned())
            .collect();
        lines.push(format!("{method_name} {}", arg.source()));
        // `arg == node.first_argument` is structural.
        if first.is_some_and(|f| f.same_as(arg)) {
            out.extend(lines);
        } else {
            out.extend(lines.into_iter().map(|l| format!("{indent}{l}")));
        }
    }
    out.join("\n")
}

fn range_with_trailing_argument_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let end = node.source_range().end_pos;
    let comment = node
        .arguments()
        .iter()
        .filter_map(|argument| ps.node_comments(*argument).last().map(|c| c.range))
        .find(|range| range.begin_pos >= end);
    match comment {
        Some(comment) => node.source_range().join(comment),
        None => node.source_range(),
    }
}
