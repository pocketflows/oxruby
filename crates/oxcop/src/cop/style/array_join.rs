//! Port of `lib/rubocop/cop/style/array_join.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Favor `Array#join` over `Array#*`.";

static JOIN_CANDIDATE: NodePattern = NodePattern::new("(send $array :* $str)");

#[derive(Default)]
pub struct ArrayJoin;

impl Cop for ArrayJoin {
    fn name(&self) -> &'static str {
        "Style/ArrayJoin"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["*"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = JOIN_CANDIDATE.match_node(node) else {
            return;
        };
        let array = captures[0].node().unwrap();
        let join_arg = captures[1].node().unwrap();
        cx.add_offense_with_correction(node.loc().selector.unwrap(), MSG, |corrector| {
            corrector.replace(
                node.source_range(),
                format!("{}.join({})", array.source(), join_arg.source()),
            );
        });
    }
}
