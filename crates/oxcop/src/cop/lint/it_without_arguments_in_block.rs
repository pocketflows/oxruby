//! Port of `lib/rubocop/cop/lint/it_without_arguments_in_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "`it` calls without arguments will refer to the first block param in Ruby 3.4; use `it()` or `self.it`.";

#[derive(Default)]
pub struct ItWithoutArgumentsInBlock;

impl Cop for ItWithoutArgumentsInBlock {
    fn name(&self) -> &'static str {
        "Lint/ItWithoutArgumentsInBlock"
    }

    fn maximum_target_ruby_version(&self) -> Option<f64> {
        Some(3.3)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["it"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(block_node) = node.ancestors().find(|a| a.is(NodeType::Block)) else {
            return;
        };
        // `arguments.empty_and_without_delimiters?`
        let Some(arguments) = block_node.child(1) else {
            return;
        };
        if !(arguments.children().is_empty() && arguments.loc().expression.is_none()) {
            return;
        }
        if deprecated_it_method(node) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}

fn deprecated_it_method(node: Node<'_>) -> bool {
    node.receiver().is_none()
        && node.arguments().is_empty()
        && !node.is_parenthesized()
        && !node.is_block_literal()
}
