//! Port of `lib/rubocop/cop/style/each_for_simple_loop.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `Integer#times` for a simple loop which iterates a fixed number of times.";

static EACH_RANGE: NodePattern = NodePattern::new(
    "(block
       (call
         (begin
           ($range (int $_) (int $_)))
         :each)
       (args ...)
       ...)",
);

static EACH_RANGE_WITH_ZERO_ORIGIN: NodePattern = NodePattern::new(
    "(block
       (call
         (begin
           (range (int 0) (int _)))
         :each)
       (args ...)
       ...)",
);

static EACH_RANGE_WITHOUT_BLOCK_ARGUMENT: NodePattern = NodePattern::new(
    "(block
       (call
         (begin
           (range (int _) (int _)))
         :each)
       (args)
       ...)",
);

#[derive(Default)]
pub struct EachForSimpleLoop;

impl Cop for EachForSimpleLoop {
    fn name(&self) -> &'static str {
        "Style/EachForSimpleLoop"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !offending(node) {
            return;
        }
        let send_node = node.send_node().unwrap();
        cx.add_offense_with_correction(send_node.source_range(), MSG, |corrector| {
            let Some(captures) = EACH_RANGE.match_node(node) else {
                return;
            };
            let range_type = captures[0].sym();
            let (Some(min), Some(mut max)) = (captures[1].val().and_then(|v| v.int()), captures[2].val().and_then(|v| v.int())) else {
                return;
            };
            if range_type == Some("irange") {
                max += 1;
            }
            corrector.replace(send_node.source_range(), format!("{}.times", max - min));
        });
    }
}

fn offending(node: Node<'_>) -> bool {
    if !node.arguments().is_empty() {
        return false;
    }
    EACH_RANGE_WITH_ZERO_ORIGIN.is_match(node) || EACH_RANGE_WITHOUT_BLOCK_ARGUMENT.is_match(node)
}
