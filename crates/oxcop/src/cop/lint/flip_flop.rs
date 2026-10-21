//! Port of `lib/rubocop/cop/lint/flip_flop.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid the use of flip-flop operators.";

#[derive(Default)]
pub struct FlipFlop;

impl Cop for FlipFlop {
    fn name(&self) -> &'static str {
        "Lint/FlipFlop"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Iflipflop, NodeType::Eflipflop]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        cx.add_offense(node.source_range(), MSG);
    }
}
