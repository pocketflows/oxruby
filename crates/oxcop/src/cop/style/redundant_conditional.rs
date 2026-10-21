//! Port of `lib/rubocop/cop/style/redundant_conditional.rb`.

use crate::ast::node::COMPARISON_OPERATORS;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::indentation;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct RedundantConditional;

impl Cop for RedundantConditional {
    fn name(&self) -> &'static str {
        "Style/RedundantConditional"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_modifier_form() {
            return;
        }
        let Some(inverted) = redundant(node) else {
            return;
        };
        let condition = node.condition().unwrap().source();
        let expression = if inverted {
            format!("!({condition})")
        } else {
            condition.to_owned()
        };
        let replacement = if node.is_elsif() {
            format!("else\n{}{expression}", indentation(cx, node))
        } else {
            expression
        };
        let msg = if node.is_elsif() {
            format!("\n{replacement}")
        } else {
            replacement.clone()
        };
        let message = format!("This conditional expression can just be replaced by `{msg}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), &replacement);
        });
    }
}

/// `redundant_condition?` (`Some(false)`) or `redundant_condition_inverted?`
/// (`Some(true)`): `(if (send _ COMPARISON _) true false)`.
fn redundant(node: Node<'_>) -> Option<bool> {
    let condition = node.child(0)?;
    if !(condition.is_send()
        && condition.children().len() == 3
        && condition
            .method_name()
            .is_some_and(|m| COMPARISON_OPERATORS.contains(&m)))
    {
        return None;
    }
    match (node.child(1)?.ty(), node.child(2)?.ty()) {
        (NodeType::True, NodeType::False) => Some(false),
        (NodeType::False, NodeType::True) => Some(true),
        _ => None,
    }
}
