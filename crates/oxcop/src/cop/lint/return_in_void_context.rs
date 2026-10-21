//! Port of `lib/rubocop/cop/lint/return_in_void_context.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const SCOPE_CHANGING_METHODS: &[&str] = &["lambda", "define_method", "define_singleton_method"];

#[derive(Default)]
pub struct ReturnInVoidContext;

impl Cop for ReturnInVoidContext {
    fn name(&self) -> &'static str {
        "Lint/ReturnInVoidContext"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Return]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, return_node: Node<'a>) {
        if return_node.descendants().next().is_none() {
            return;
        }
        let Some(def_node) = return_node.ancestors().find(|a| a.is_any_def()) else {
            return;
        };
        if !def_node.is_void_context() {
            return;
        }
        if return_node.ancestors().any(|block_node| {
            block_node.is_any_block()
                && block_node
                    .method_name()
                    .is_some_and(|m| SCOPE_CHANGING_METHODS.contains(&m))
        }) {
            return;
        }

        let Some(keyword) = return_node.loc().keyword else {
            return;
        };
        let message = format!(
            "Do not return a value in `{}`.",
            def_node.method_name().unwrap_or_default()
        );
        cx.add_offense(keyword, &message);
    }
}
