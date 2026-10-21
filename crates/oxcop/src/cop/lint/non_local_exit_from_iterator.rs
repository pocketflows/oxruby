//! Port of `lib/rubocop/cop/lint/non_local_exit_from_iterator.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Non-local exit from iterator, without return value. `next`, `break`, `Array#find`, `Array#any?`, etc. is preferred.";

static CHAINED_SEND: NodePattern = NodePattern::new("(call !nil? ...)");
static DEFINE_METHOD: NodePattern =
    NodePattern::new("(send _ {:define_method :define_singleton_method} _)");

#[derive(Default)]
pub struct NonLocalExitFromIterator;

impl Cop for NonLocalExitFromIterator {
    fn name(&self) -> &'static str {
        "Lint/NonLocalExitFromIterator"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Return]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, return_node: Node<'a>) {
        if return_value(return_node) {
            return;
        }
        for node in return_node
            .ancestors()
            .filter(|a| a.is_any_block() || a.is_any_def())
        {
            if scoped_node(node) {
                break;
            }
            let send_node = node.send_node();
            if send_node.is_some_and(|s| DEFINE_METHOD.is_match(s)) {
                break;
            }
            if argument_list_empty(node) {
                continue;
            }
            if send_node.is_some_and(|s| CHAINED_SEND.is_match(s)) {
                cx.add_offense(return_node.loc().keyword.unwrap(), MSG);
                break;
            }
        }
    }
}

fn scoped_node(node: Node<'_>) -> bool {
    node.is_any_def() || node.is_lambda()
}

fn return_value(return_node: Node<'_>) -> bool {
    !return_node.children().is_empty()
}

/// `node.argument_list.empty?` for a block.
fn argument_list_empty(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::Numblock | NodeType::Itblock => false,
        _ => !node
            .child(1)
            .is_some_and(|args| args.descendants().any(|d| d.is_argument_type())),
    }
}
