//! Port of `lib/rubocop/cop/lint/float_out_of_range.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Float out of range.";

#[derive(Default)]
pub struct FloatOutOfRange;

impl Cop for FloatOutOfRange {
    fn name(&self) -> &'static str {
        "Lint/FloatOutOfRange"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Float]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(Child::Float(value)) = node.value() else {
            return;
        };
        if !(value.is_infinite()
            || (*value == 0.0 && node.source().bytes().any(|b| (b'1'..=b'9').contains(&b))))
        {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}
