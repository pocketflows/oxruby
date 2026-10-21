//! Port of `lib/rubocop/cop/style/implicit_runtime_error.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static IMPLICIT_RUNTIME_ERROR_RAISE_OR_FAIL: NodePattern =
    NodePattern::new("(send nil? ${:raise :fail} {str dstr})");

#[derive(Default)]
pub struct ImplicitRuntimeError;

impl Cop for ImplicitRuntimeError {
    fn name(&self) -> &'static str {
        "Style/ImplicitRuntimeError"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["raise", "fail"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(method) = IMPLICIT_RUNTIME_ERROR_RAISE_OR_FAIL
            .match_node(node)
            .and_then(|c| c[0].sym())
        else {
            return;
        };
        let message = format!(
            "Use `{method}` with an explicit exception class and message, rather than just a message."
        );
        cx.add_offense(node.source_range(), &message);
    }
}
