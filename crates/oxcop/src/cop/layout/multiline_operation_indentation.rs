//! Port of `lib/rubocop/cop/layout/multiline_operation_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::check_assignment::extract_rhs;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::multiline_expression_indentation::{
    self as mei, ArgumentKind, MultilineExpressionIndentation, argument_in_method_call, check,
    correct_indentation, indentation, kw_node_with_special_indentation, not_for_this_cop,
    operation_description, part_of_assignment_rhs,
};
use crate::cop::mixin::util::begins_its_line;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct MultilineOperationIndentation {
    column_delta: isize,
}

impl Cop for MultilineOperationIndentation {
    fn name(&self) -> &'static str {
        "Layout/MultilineOperationIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::And, NodeType::Or, NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_call() {
            mei::on_send(self, cx, node);
            return;
        }
        // `check_and_or(node)`.
        let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
            return;
        };
        let rhs = rhs.source_range();
        let style = style(cx);
        let range = self.offending_range(cx, node, lhs, rhs, style);
        check(self, cx, range, node, lhs, rhs);
    }
}

fn should_align(cx: &Investigation<'_, '_>, node: Node<'_>, rhs: Range, given_style: &str) -> bool {
    let assignment_node = part_of_assignment_rhs(node, Some(rhs));
    if let Some(assignment_node) = assignment_node
        && let Some(assignment_rhs) = extract_rhs(assignment_node)
        && begins_its_line(cx.processed_source, assignment_rhs.source_range())
    {
        return true;
    }
    if given_style != "aligned" {
        return false;
    }
    if kw_node_with_special_indentation(node).is_some() || assignment_node.is_some() {
        return true;
    }
    argument_in_method_call(node, ArgumentKind::WithOrWithoutParentheses)
        .is_some_and(|call| !call.is_def_modifier())
}

impl MultilineExpressionIndentation for MultilineOperationIndentation {
    fn relevant_node(&self, node: Node<'_>) -> bool {
        if node.is_send() && node.is_unary_operation() {
            return false;
        }
        node.loc().dot.is_none()
    }

    fn right_hand_side(&self, _cx: &Investigation<'_, '_>, send_node: Node<'_>) -> Option<Range> {
        send_node.first_argument().map(|a| a.source_range())
    }

    fn offending_range<'a>(
        &mut self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
        given_style: &str,
    ) -> Option<Range> {
        let ps = cx.processed_source;
        if !begins_its_line(ps, rhs) {
            return None;
        }
        if not_for_this_cop(node) {
            return None;
        }
        let correct_column = if should_align(cx, node, rhs, given_style) {
            node.column()
        } else {
            indentation(ps, lhs) + correct_indentation(cx, node)
        };
        self.column_delta = correct_column as isize - ps.column_of(rhs.begin_pos) as isize;
        (self.column_delta != 0).then_some(rhs)
    }

    fn message<'a>(
        &self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
    ) -> String {
        let ps = cx.processed_source;
        let what = operation_description(node, rhs);
        if should_align(cx, node, rhs, style(cx)) {
            format!("Align the operands of {what} spanning multiple lines.")
        } else {
            let used_indentation =
                ps.column_of(rhs.begin_pos) as isize - indentation(ps, lhs) as isize;
            format!(
                "Use {} (not {used_indentation}) spaces for indenting {what} spanning multiple lines.",
                correct_indentation(cx, node)
            )
        }
    }

    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, range: Range) {
        alignment_corrector::correct(
            corrector,
            cx.processed_source,
            cx.config,
            Some(Target::Range(range)),
            self.column_delta,
            false,
        );
    }
}
