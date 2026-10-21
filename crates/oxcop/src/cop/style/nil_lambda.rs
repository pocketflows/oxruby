//! Port of `lib/rubocop/cop/style/nil_lambda.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{SurroundingSpace, range_by_whole_lines, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};

static NIL_RETURN: NodePattern = NodePattern::new("{ ({return next break} nil) (nil) }");

#[derive(Default)]
pub struct NilLambda;

impl Cop for NilLambda {
    fn name(&self) -> &'static str {
        "Style/NilLambda"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_lambda() || node.is_proc()) {
            return;
        }
        let Some(body) = node.body() else {
            return;
        };
        if !NIL_RETURN.is_match(body) {
            return;
        }
        if body.is(NodeType::Return) && !node.is_lambda() {
            return;
        }
        let kind = if node.is_lambda() { "lambda" } else { "proc" };
        let message = format!("Use an empty {kind} instead of always returning nil.");
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let range = if node.is_single_line() {
                range_with_surrounding_space(
                    ps.buffer_source(),
                    body.source_range(),
                    SurroundingSpace::default(),
                )
            } else {
                range_by_whole_lines(ps, body.source_range(), true)
            };
            corrector.remove(range);
        });
    }
}
