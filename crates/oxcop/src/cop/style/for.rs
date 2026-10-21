//! Port of `lib/rubocop/cop/style/for.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::each_to_for_corrector::EachToForCorrector;
use crate::cop::correctors::for_to_each_corrector::ForToEachCorrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const PREFER_EACH: &str = "Prefer `each` over `for`.";
const PREFER_FOR: &str = "Prefer `for` over `each`.";

#[derive(Default)]
pub struct For;

impl Cop for For {
    fn name(&self) -> &'static str {
        "Style/For"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::For,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::For) {
            if style(cx) == "each" {
                cx.add_offense_with_correction(node.source_range(), PREFER_EACH, |corrector| {
                    ForToEachCorrector::new(node).call(corrector);
                });
            }
            return;
        }
        if !suspect_enumerable(node) {
            return;
        }
        if style(cx) == "for" {
            if node.receiver().is_none() || rescue_or_ensure_body(node) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), PREFER_FOR, |corrector| {
                EachToForCorrector::new(node).call(corrector);
            });
        }
    }
}

fn suspect_enumerable(node: Node<'_>) -> bool {
    node.is_multiline()
        && node.is_method("each")
        && node.send_node().is_some_and(|s| !s.has_arguments())
}

fn rescue_or_ensure_body(node: Node<'_>) -> bool {
    node.body()
        .is_some_and(|b| b.is_any(&[NodeType::Rescue, NodeType::Ensure]))
}
