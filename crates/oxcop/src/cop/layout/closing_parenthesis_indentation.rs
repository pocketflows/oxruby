//! Port of `lib/rubocop/cop/layout/closing_parenthesis_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::util::begins_its_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_ALIGN: &str = "Align `)` with `(`.";

#[derive(Default)]
pub struct ClosingParenthesisIndentation;

impl Cop for ClosingParenthesisIndentation {
    fn name(&self) -> &'static str {
        "Layout/ClosingParenthesisIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Send, Csend, Begin, Def, Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Send | NodeType::Csend => check(cx, node, &node.arguments()),
            NodeType::Begin => check(cx, node, &node.child_nodes().collect::<Vec<_>>()),
            _ => {
                if let Some(args) = node.arguments_node() {
                    check(cx, args, &node.arguments());
                }
            }
        }
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>, elements: &[Node<'_>]) {
    if elements.is_empty() {
        check_for_no_elements(cx, node);
    } else {
        check_for_elements(cx, node, elements);
    }
}

fn check_for_elements(cx: &mut Investigation<'_, '_>, node: Node<'_>, elements: &[Node<'_>]) {
    let ps = cx.processed_source;
    let (Some(left_paren), Some(right_paren)) = (node.loc().begin, node.loc().end) else {
        return;
    };
    if !begins_its_line(ps, right_paren) {
        return;
    }
    let correct_column = expected_column(cx, left_paren, elements);
    let column_delta = correct_column as isize - ps.column_of(right_paren.begin_pos) as isize;
    if column_delta == 0 {
        return;
    }
    register(cx, correct_column, left_paren, right_paren, column_delta);
}

fn check_for_no_elements(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let ps = cx.processed_source;
    let Some(right_paren) = node.loc().end else {
        return;
    };
    if !begins_its_line(ps, right_paren) {
        return;
    }
    let Some(left_paren) = node.loc().begin else {
        return;
    };
    let candidates = [
        ps.line_indentation(ps.line_of(left_paren.begin_pos)),
        ps.column_of(left_paren.begin_pos),
        node.column(),
    ];
    let right_column = ps.column_of(right_paren.begin_pos);
    if candidates.contains(&right_column) {
        return;
    }
    let correct_column = candidates[0];
    let column_delta = correct_column as isize - right_column as isize;
    register(cx, correct_column, left_paren, right_paren, column_delta);
}

fn register(
    cx: &mut Investigation<'_, '_>,
    correct_column: usize,
    left_paren: Range,
    right_paren: Range,
    column_delta: isize,
) {
    let ps = cx.processed_source;
    let message = if correct_column == ps.column_of(left_paren.begin_pos) {
        MSG_ALIGN.to_owned()
    } else {
        format!(
            "Indent `)` to column {correct_column} (not {})",
            ps.column_of(right_paren.begin_pos)
        )
    };
    cx.add_offense_cx(right_paren, &message, |corrector, cx| {
        alignment_corrector::correct(
            corrector,
            cx.processed_source,
            cx.config,
            Some(Target::Range(right_paren)),
            column_delta,
            false,
        );
    });
}

fn expected_column(cx: &Investigation<'_, '_>, left_paren: Range, elements: &[Node<'_>]) -> usize {
    let ps = cx.processed_source;
    if line_break_after_left_paren(cx, left_paren, elements) {
        let source_indent = ps.line_indentation(elements[0].first_line()) as isize;
        let new_indent = source_indent - configured_indentation_width(cx) as isize;
        new_indent.max(0) as usize
    } else if all_elements_aligned(elements) {
        ps.column_of(left_paren.begin_pos)
    } else {
        ps.line_indentation(elements[0].first_line())
    }
}

fn all_elements_aligned(elements: &[Node<'_>]) -> bool {
    let columns: Vec<usize> = if elements[0].is(NodeType::Hash) {
        elements[0].child_nodes().map(|c| c.column()).collect()
    } else {
        elements.iter().map(|e| e.column()).collect()
    };
    let mut unique = columns.clone();
    unique.sort_unstable();
    unique.dedup();
    unique.len() == 1
}

fn line_break_after_left_paren(cx: &Investigation<'_, '_>, left_paren: Range, elements: &[Node<'_>]) -> bool {
    elements
        .first()
        .is_some_and(|e| e.first_line() > cx.processed_source.line_of(left_paren.begin_pos))
}
