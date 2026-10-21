//! Port of `lib/rubocop/cop/layout/line_end_string_concatenation_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG_ALIGN: &str = "Align parts of a string concatenated with backslash.";
const MSG_INDENT: &str = "Indent the first part of a string concatenated with backslash.";

#[derive(Default)]
pub struct LineEndStringConcatenationIndentation;

impl Cop for LineEndStringConcatenationIndentation {
    fn name(&self) -> &'static str {
        "Layout/LineEndStringConcatenationIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !strings_concatenated_with_backslash(node) {
            return;
        }
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        if children.is_empty() {
            return;
        }
        if style(cx) == "aligned" && !always_indented(node) {
            check_aligned(cx, &children, 1);
        } else {
            check_indented(cx, &children);
            check_aligned(cx, &children, 2);
        }
    }
}

fn strings_concatenated_with_backslash(dstr_node: Node<'_>) -> bool {
    dstr_node.is_multiline()
        && dstr_node
            .child_nodes()
            .all(|c| matches!(c.ty(), NodeType::Str | NodeType::Dstr))
        && dstr_node.children().len() == dstr_node.child_nodes().count()
        && !dstr_node.child_nodes().any(|c| c.is_multiline())
}

/// `PARENT_TYPES_FOR_INDENTED.include?(dstr_node.parent&.type)`.
fn always_indented(dstr_node: Node<'_>) -> bool {
    match dstr_node.parent() {
        None => true,
        Some(parent) => matches!(
            parent.ty(),
            NodeType::Block | NodeType::Begin | NodeType::Def | NodeType::Defs | NodeType::If
        ),
    }
}

fn check_aligned(cx: &mut Investigation<'_, '_>, children: &[Node<'_>], start_index: usize) {
    let mut base_column = children[start_index - 1].column() as isize;
    for &child in children.iter().skip(start_index) {
        let column_delta = base_column - child.column() as isize;
        if column_delta != 0 {
            add_offense_and_correction(cx, child, MSG_ALIGN, column_delta);
        }
        base_column = child.column() as isize;
    }
}

fn check_indented(cx: &mut Investigation<'_, '_>, children: &[Node<'_>]) {
    let column_delta = base_column(cx, children[0]) as isize
        + configured_indentation_width(cx) as isize
        - children[1].column() as isize;
    if column_delta != 0 {
        add_offense_and_correction(cx, children[1], MSG_INDENT, column_delta);
    }
}

fn base_column(cx: &Investigation<'_, '_>, child: Node<'_>) -> usize {
    match child.parent().and_then(|p| p.parent()) {
        Some(grandparent) if grandparent.is(NodeType::Pair) => grandparent.column(),
        _ => {
            let ps = cx.processed_source;
            ps.source_line(child.first_line())
                .chars()
                .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
                .unwrap_or(0)
        }
    }
}

fn add_offense_and_correction(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    message: &str,
    column_delta: isize,
) {
    cx.add_offense_cx(node.source_range(), message, |corrector, cx| {
        alignment_corrector::correct(
            corrector,
            cx.processed_source,
            cx.config,
            Some(Target::Node(node)),
            column_delta,
            false,
        );
    });
}
