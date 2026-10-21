//! Port of `lib/rubocop/cop/lint/rand_one.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static RAND_ONE: NodePattern = NodePattern::new(
    "(send {(const {nil? cbase} :Kernel) nil?} :rand {(int {-1 1}) (float {-1.0 1.0})})",
);

#[derive(Default)]
pub struct RandOne;

impl Cop for RandOne {
    fn name(&self) -> &'static str {
        "Lint/RandOne"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["rand"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !RAND_ONE.is_match(node) {
            return;
        }

        let message = format!(
            "`{}` always returns `0`. Perhaps you meant `rand(2)` or `rand`?",
            node.source()
        );
        cx.add_offense(node.source_range(), &message);
    }
}
