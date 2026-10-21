//! Port of `lib/rubocop/cop/style/object_then.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const RESTRICT_ON_SEND: &[&str] = &["then", "yield_self"];

#[derive(Default)]
pub struct ObjectThen;

impl Cop for ObjectThen {
    fn name(&self) -> &'static str {
        "Style/ObjectThen"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Send,
            NodeType::Csend,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            if !node
                .method_name()
                .is_some_and(|m| RESTRICT_ON_SEND.contains(&m))
            {
                return;
            }
            check_method_node(cx, node.send_node().unwrap());
        } else {
            let args = node.arguments();
            if !(args.len() == 1 && args[0].is(NodeType::BlockPass)) {
                return;
            }
            check_method_node(cx, node);
        }
    }
}

fn check_method_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let style = style(cx);
    if node.is_method(style) {
        return;
    }
    let message = format!(
        "Prefer `{style}` over `{}`.",
        node.method_name().unwrap_or("")
    );
    let selector = node.loc().selector.unwrap();
    cx.add_offense_with_correction(selector, &message, |corrector| {
        let prefer = if style == "then" && node.receiver().is_none() {
            "self.then"
        } else {
            style
        };
        corrector.replace(selector, prefer);
    });
}
