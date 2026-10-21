//! Port of `lib/rubocop/cop/lint/empty_expression.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid empty expressions.";

#[derive(Default)]
pub struct EmptyExpression;

impl Cop for EmptyExpression {
    fn name(&self) -> &'static str {
        "Lint/EmptyExpression"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Begin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !empty_expression(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn empty_expression(begin_node: Node<'_>) -> bool {
    begin_node.children().is_empty()
}
