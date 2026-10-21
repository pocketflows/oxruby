//! Port of `lib/rubocop/cop/style/class_check.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct ClassCheck;

impl Cop for ClassCheck {
    fn name(&self) -> &'static str {
        "Style/ClassCheck"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["is_a?", "kind_of?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if Some(style(cx)) == node.method_name() {
            return;
        }
        let (prefer, current) = if node.is_method("is_a?") {
            ("kind_of?", "is_a?")
        } else {
            ("is_a?", "kind_of?")
        };
        let message = format!("Prefer `Object#{prefer}` over `Object#{current}`.");
        let selector = node.loc().selector.unwrap();
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(selector, prefer);
        });
    }
}
