//! Port of `lib/rubocop/cop/style/while_until_modifier.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::statement_modifier::{
    modifier_fits_on_single_line, non_eligible_body, non_eligible_condition, non_eligible_node,
    to_modifier_form,
};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct WhileUntilModifier;

impl Cop for WhileUntilModifier {
    fn name(&self) -> &'static str {
        "Style/WhileUntilModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !single_line_as_modifier(cx, node) {
            return;
        }
        let message = format!(
            "Favor modifier `{}` usage when having a single-line body.",
            node.keyword()
        );
        let replacement = to_modifier_form(cx, node);
        cx.add_offense_with_correction(node.loc().keyword.unwrap(), &message, |corrector| {
            corrector.replace(node.source_range(), replacement);
        });
    }
}

/// `single_line_as_modifier?`, with this cop's `non_eligible_body?`
/// (`body&.conditional? || super`).
fn single_line_as_modifier(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let body = node.body();
    if non_eligible_node(cx, node)
        || body.is_some_and(|b| b.is_conditional())
        || non_eligible_body(cx, body)
        || node.condition().is_none_or(non_eligible_condition)
    {
        return false;
    }
    modifier_fits_on_single_line(cx, node)
}
