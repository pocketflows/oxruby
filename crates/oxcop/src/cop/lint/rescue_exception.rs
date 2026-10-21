//! Port of `lib/rubocop/cop/lint/rescue_exception.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "Avoid rescuing the `Exception` class. Perhaps you meant to rescue `StandardError`?";

#[derive(Default)]
pub struct RescueException;

impl Cop for RescueException {
    fn name(&self) -> &'static str {
        "Lint/RescueException"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.exceptions().into_iter().any(targets_exception) {
            return;
        }

        cx.add_offense(node.source_range(), MSG);
    }
}

fn targets_exception(rescue_arg_node: Node<'_>) -> bool {
    rescue_arg_node.const_name().as_deref() == Some("Exception")
}
