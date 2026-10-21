//! Port of `lib/rubocop/cop/style/negated_if.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::negative_conditional::{check_negative_conditional, correct_negative_condition};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct NegatedIf;

impl Cop for NegatedIf {
    fn name(&self) -> &'static str {
        "Style/NegatedIf"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_unless() || node.is_elsif() || node.is_ternary() {
            return;
        }
        if correct_style(cx, node) {
            return;
        }
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

fn correct_style(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let style = style(cx);
    (style == "prefix" && node.is_modifier_form()) || (style == "postfix" && !node.is_modifier_form())
}
