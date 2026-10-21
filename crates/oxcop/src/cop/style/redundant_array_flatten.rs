//! Port of `lib/rubocop/cop/style/redundant_array_flatten.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove the redundant `flatten`.";

static FLATTEN_JOIN: NodePattern =
    NodePattern::new("(call (call !nil? :flatten _?) :join (nil)?)");

#[derive(Default)]
pub struct RedundantArrayFlatten;

impl Cop for RedundantArrayFlatten {
    fn name(&self) -> &'static str {
        "Style/RedundantArrayFlatten"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["flatten"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.parent().is_some_and(|p| FLATTEN_JOIN.is_match(p)) {
            return;
        }
        let range = node.loc().dot.unwrap().begin().join(node.source_range().end());
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove(range);
        });
    }
}
