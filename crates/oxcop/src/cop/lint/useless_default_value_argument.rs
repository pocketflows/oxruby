//! Port of `lib/rubocop/cop/lint/useless_default_value_argument.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_receivers::allowed_receiver;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Block supersedes default value argument.";

static DEFAULT_VALUE_ARGUMENT_AND_BLOCK: NodePattern = NodePattern::new(
    "(any_block
       {
         (call !nil? :fetch $_key $_default_value)
         (send (const _ :Array) :new $_size $_default_value)
       }
       _args
       _block_body)",
);

#[derive(Default)]
pub struct UselessDefaultValueArgument;

impl Cop for UselessDefaultValueArgument {
    fn name(&self) -> &'static str {
        "Lint/UselessDefaultValueArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["fetch", "new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(captures) = node
            .parent()
            .and_then(|p| DEFAULT_VALUE_ARGUMENT_AND_BLOCK.match_node(p))
        else {
            return;
        };
        let (Some(prev_arg_node), Some(default_value_node)) =
            (captures[0].node(), captures[1].node())
        else {
            return;
        };
        if allowed_receiver(cx, receiver) {
            return;
        }
        if hash_without_braces(default_value_node) {
            return;
        }

        cx.add_offense_with_correction(default_value_node.source_range(), MSG, |corrector| {
            corrector.remove(
                prev_arg_node
                    .source_range()
                    .end()
                    .join(default_value_node.source_range()),
            );
        });
    }
}

fn hash_without_braces(node: Node<'_>) -> bool {
    node.is(NodeType::Hash) && !node.has_braces()
}
