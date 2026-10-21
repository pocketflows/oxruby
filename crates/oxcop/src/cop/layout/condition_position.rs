//! Port of `lib/rubocop/cop/layout/condition_position.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{range_between, range_by_whole_lines};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct ConditionPosition;

impl Cop for ConditionPosition {
    fn name(&self) -> &'static str {
        "Layout/ConditionPosition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::If) && node.is_ternary() {
            return;
        }
        check(cx, node);
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.is_modifier_form() || node.is_single_line_condition() {
        return;
    }
    let Some(condition) = node.condition() else {
        return;
    };
    let message = message(condition);
    let ps = cx.processed_source;
    cx.add_offense_with_correction(condition.source_range(), &message, |corrector| {
        let keyword = condition.parent().unwrap().loc().keyword.unwrap();
        corrector.insert_after(keyword, format!(" {}", condition.source()));
        corrector.remove(removal_range(ps, node, condition));
    });
}

fn message(condition: Node<'_>) -> String {
    format!(
        "Place the condition on the same line as `{}`.",
        condition.parent().unwrap().keyword()
    )
}

fn removal_range(
    ps: &crate::processed_source::ProcessedSource<'_>,
    node: Node<'_>,
    condition: Node<'_>,
) -> Range {
    match node.body() {
        Some(body) if body.first_line() == condition.last_line() => range_between(
            condition.source_range().begin_pos,
            body.source_range().begin_pos,
        ),
        _ => range_by_whole_lines(ps, condition.source_range(), true),
    }
}
