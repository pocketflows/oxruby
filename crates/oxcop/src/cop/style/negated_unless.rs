//! Port of `lib/rubocop/cop/style/negated_unless.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::negative_conditional::{check_negative_conditional, correct_negative_condition};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct NegatedUnless;

impl Cop for NegatedUnless {
    fn name(&self) -> &'static str {
        "Style/NegatedUnless"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_if() || node.is_elsif() || node.is_ternary() {
            return;
        }
        let style = style(cx);
        if (style == "prefix" && node.is_modifier_form())
            || (style == "postfix" && !node.is_modifier_form())
        {
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
