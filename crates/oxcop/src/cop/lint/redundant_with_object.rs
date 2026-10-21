//! Port of `lib/rubocop/cop/lint/redundant_with_object.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_EACH_WITH_OBJECT: &str = "Use `each` instead of `each_with_object`.";
const MSG_WITH_OBJECT: &str = "Remove redundant `with_object`.";

static REDUNDANT_WITH_OBJECT: NodePattern = NodePattern::new(
    "{
       (block
         $(call _ {:each_with_object :with_object} _) (args (arg _)) ...)
       (numblock
         $(call _ {:each_with_object :with_object} _) 1 ...)
       (itblock
         $(call _ {:each_with_object :with_object} _) _ ...)
     }",
);

#[derive(Default)]
pub struct RedundantWithObject;

impl Cop for RedundantWithObject {
    fn name(&self) -> &'static str {
        "Lint/RedundantWithObject"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(send) = REDUNDANT_WITH_OBJECT
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };

        let range = with_object_range(send);
        let message = if send.is_method("each_with_object") {
            MSG_EACH_WITH_OBJECT
        } else {
            MSG_WITH_OBJECT
        };
        cx.add_offense_with_correction(range, message, |corrector| {
            if send.is_method("each_with_object") {
                corrector.replace(range, "each");
            } else {
                corrector.remove(range);
                if let Some(dot) = send.loc().dot {
                    corrector.remove(dot);
                }
            }
        });
    }
}

fn with_object_range(send: Node<'_>) -> Range {
    range_between(
        send.loc().selector.unwrap_or_default().begin_pos,
        send.source_range().end_pos,
    )
}
