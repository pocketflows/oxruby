//! Port of `lib/rubocop/cop/style/reverse_find.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `rfind` instead.";

static REVERSE_FIND: NodePattern = NodePattern::new(
    "(call (call _ {:reverse :reverse_each}) {:find :detect} (block_pass sym)?)",
);

#[derive(Default)]
pub struct ReverseFind;

impl Cop for ReverseFind {
    fn name(&self) -> &'static str {
        "Style/ReverseFind"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(4.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["find", "detect"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !REVERSE_FIND.is_match(node) {
            return;
        }
        let range = node
            .child(0)
            .unwrap()
            .loc()
            .selector
            .unwrap()
            .join(node.loc().selector.unwrap());
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(range, "rfind");
        });
    }
}
