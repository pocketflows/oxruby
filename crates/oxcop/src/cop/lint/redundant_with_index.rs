//! Port of `lib/rubocop/cop/lint/redundant_with_index.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_EACH_WITH_INDEX: &str = "Use `each` instead of `each_with_index`.";
const MSG_WITH_INDEX: &str = "Remove redundant `with_index`.";

static REDUNDANT_WITH_INDEX: NodePattern = NodePattern::new(
    "{
       (block
         $(call _ {:each_with_index :with_index} ...)
         {(args (arg _)) (args)} ...)
       (numblock
         $(call _ {:each_with_index :with_index} ...) 1 ...)
       (itblock
         $(call _ {:each_with_index :with_index} ...) _ ...)
     }",
);

#[derive(Default)]
pub struct RedundantWithIndex;

impl Cop for RedundantWithIndex {
    fn name(&self) -> &'static str {
        "Lint/RedundantWithIndex"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        if node.is_method("with_index") && receiver.receiver().is_none() {
            return;
        }
        let Some(send) = REDUNDANT_WITH_INDEX
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };

        let range = with_index_range(send);
        let message = if send.is_method("each_with_index") {
            MSG_EACH_WITH_INDEX
        } else {
            MSG_WITH_INDEX
        };
        cx.add_offense_with_correction(range, message, |corrector| {
            if send.is_method("each_with_index") {
                if let Some(selector) = send.loc().selector {
                    corrector.replace(selector, "each");
                }
            } else {
                corrector.remove(range);
                if let Some(dot) = send.loc().dot {
                    corrector.remove(dot);
                }
            }
        });
    }
}

fn with_index_range(send: Node<'_>) -> Range {
    range_between(
        send.loc().selector.unwrap_or_default().begin_pos,
        send.source_range().end_pos,
    )
}
