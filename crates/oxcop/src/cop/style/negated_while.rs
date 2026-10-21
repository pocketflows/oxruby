//! Port of `lib/rubocop/cop/style/negated_while.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::negative_conditional::{check_negative_conditional, correct_negative_condition};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct NegatedWhile;

impl Cop for NegatedWhile {
    fn name(&self) -> &'static str {
        "Style/NegatedWhile"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let message = format!(
            "Favor `{}` over `{}` for negative conditions.",
            node.inverse_keyword(),
            node.keyword()
        );
        check_negative_conditional(cx, node, &message, |corrector| {
            correct_negative_condition(corrector, node);
        });
    }
}
