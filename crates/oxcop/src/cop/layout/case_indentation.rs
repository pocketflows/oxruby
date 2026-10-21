//! Port of `lib/rubocop/cop/layout/case_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct CaseIndentation;

impl Cop for CaseIndentation {
    fn name(&self) -> &'static str {
        "Layout/CaseIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Case, NodeType::CaseMatch]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_single_line() {
            return;
        }
        if enforced_style_end(cx) && end_and_last_conditional_same_line(cx.processed_source, node) {
            return;
        }
        let (branch_type, branches) = if node.is(NodeType::Case) {
            ("when", node.when_branches())
        } else {
            (
                "in",
                node.child_nodes()
                    .filter(|c| c.is(NodeType::InPattern))
                    .collect(),
            )
        };
        for when_node in branches {
            check_when(cx, when_node, branch_type);
        }
    }
}

fn end_and_last_conditional_same_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let Some(end) = node.loc().end else {
        return false;
    };
    let last_conditional_line = match node.loc().else_ {
        Some(else_) => Some(ps.line_of(else_.begin_pos)),
        None => node
            .child_nodes()
            .last()
            .and_then(|last| last.loc().begin)
            .map(|b| ps.line_of(b.begin_pos)),
    };
    last_conditional_line == Some(ps.line_of(end.begin_pos))
}

fn enforced_style_end(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("end")
}

fn check_when(cx: &mut Investigation<'_, '_>, when_node: Node<'_>, branch_type: &str) {
    let ps = cx.processed_source;
    let keyword = when_node.loc().keyword.unwrap();
    let when_column = ps.column_of(keyword.begin_pos);
    let base_column = base_column(ps, when_node.parent().unwrap(), style(cx));
    if Some(when_column) == base_column.map(|b| b + indentation_width(cx)) {
        return;
    }
    incorrect_style(cx, when_node, branch_type);
}

fn indent_one_step(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IndentOneStep", false)
}

fn indentation_width(cx: &Investigation<'_, '_>) -> usize {
    if indent_one_step(cx) {
        configured_indentation_width(cx)
    } else {
        0
    }
}

fn incorrect_style(cx: &mut Investigation<'_, '_>, when_node: Node<'_>, branch_type: &str) {
    let depth = if indent_one_step(cx) {
        "one step more than"
    } else {
        "as deep as"
    };
    let message = format!("Indent `{branch_type}` {depth} `{}`.", style(cx));
    let keyword = when_node.loc().keyword.unwrap();
    cx.add_offense_cx(keyword, &message, |corrector, cx| {
        let ps = cx.processed_source;
        let whitespace = whitespace_range(ps, when_node);
        if ps.slice(whitespace).trim().is_empty() {
            corrector.replace(whitespace, replacement(cx, when_node));
        }
    });
}

fn base_column(ps: &ProcessedSource<'_>, case_node: Node<'_>, base: &str) -> Option<usize> {
    match base {
        "case" => case_node.loc().keyword.map(|k| ps.column_of(k.begin_pos)),
        "end" => case_node.loc().end.map(|e| ps.column_of(e.begin_pos)),
        _ => None,
    }
}

fn whitespace_range(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let keyword = node.loc().keyword.unwrap();
    let begin_pos = keyword.begin_pos;
    let line_start = ps.line_start(ps.line_of(begin_pos));
    range_between(line_start, begin_pos)
}

fn replacement(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let case_node = node
        .ancestors()
        .find(|a| a.is_any(&[NodeType::Case, NodeType::CaseMatch]))
        .unwrap();
    let base_type = if enforced_style_end(cx) { "end" } else { "case" };
    let column = base_column(cx.processed_source, case_node, base_type).unwrap_or(0)
        + indentation_width(cx);
    " ".repeat(column)
}
