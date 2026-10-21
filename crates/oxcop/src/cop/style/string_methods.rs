//! Port of `lib/rubocop/cop/style/string_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::method_preference::preferred_method;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct StringMethods;

impl Cop for StringMethods {
    fn name(&self) -> &'static str {
        "Style/StringMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method_name = node.method_name().unwrap_or_default();
        let Some(preferred) = preferred_method(cx, method_name) else {
            return;
        };
        let Some(selector) = node.loc().selector else {
            return;
        };
        let message = format!("Prefer `{preferred}` over `{method_name}`.");
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(selector, &preferred);
        });
    }
}
