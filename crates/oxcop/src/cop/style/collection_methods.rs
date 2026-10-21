//! Port of `lib/rubocop/cop/style/collection_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::method_preference::preferred_method;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct CollectionMethods;

impl Cop for CollectionMethods {
    fn name(&self) -> &'static str {
        "Style/CollectionMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
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

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            check_method_node(cx, node.send_node().unwrap());
        } else if implicit_block(cx, node) {
            check_method_node(cx, node);
        }
    }
}

fn check_method_node(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(method_name) = node.method_name() else {
        return;
    };
    let Some(prefer) = preferred_method(cx, method_name) else {
        return;
    };
    let Some(selector) = node.loc().selector else {
        return;
    };
    let message = format!("Prefer `{prefer}` over `{method_name}`.");
    let replacement = preferred_method(cx, node.range_source(selector)).unwrap_or_default();
    cx.add_offense_with_correction(selector, &message, |corrector| {
        corrector.replace(selector, replacement);
    });
}

fn implicit_block(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(last) = node.last_argument() else {
        return false;
    };
    last.is(NodeType::BlockPass)
        || (last.is(NodeType::Sym)
            && cx
                .cop_config_list("MethodsAcceptingSymbol")
                .contains(&node.method_name().unwrap_or("")))
}
