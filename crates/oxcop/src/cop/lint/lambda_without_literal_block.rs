//! Port of `lib/rubocop/cop/lint/lambda_without_literal_block.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "lambda without a literal block is deprecated; use the proc without lambda instead.";

static LAMBDA_WITH_SYMBOL_PROC: NodePattern =
    NodePattern::new("(send nil? :lambda (block_pass (sym _)))");

#[derive(Default)]
pub struct LambdaWithoutLiteralBlock;

impl Cop for LambdaWithoutLiteralBlock {
    fn name(&self) -> &'static str {
        "Lint/LambdaWithoutLiteralBlock"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["lambda"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.parent().is_some_and(|p| p.is(NodeType::Block))
            || node.first_argument().is_none()
            || LAMBDA_WITH_SYMBOL_PROC.is_match(node)
        {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let source = node.first_argument().unwrap().source();
            corrector.replace(
                node.source_range(),
                source.strip_prefix('&').unwrap_or(source),
            );
        });
    }
}
