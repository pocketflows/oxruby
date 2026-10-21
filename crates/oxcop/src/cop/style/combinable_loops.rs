//! Port of `lib/rubocop/cop/style/combinable_loops.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Corrector, Cop, Investigation};

const MSG: &str = "Combine this loop with the previous loop.";

#[derive(Default)]
pub struct CombinableLoops;

impl Cop for CombinableLoops {
    fn name(&self) -> &'static str {
        "Style/CombinableLoops"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::For,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.parent().is_some_and(|p| p.is(NodeType::Begin)) {
            return;
        }
        let sibling = node.left_sibling();
        if node.is(NodeType::For) {
            let Some(sibling) = sibling.filter(|s| same_collection_looping_for(node, *s)) else {
                return;
            };
            if node.body().is_none() || sibling.body().is_none() {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                if !same_child(node.child(0), sibling.child(0)) {
                    return;
                }
                combine_with_left_sibling(corrector, node, sibling);
            });
        } else {
            if !collection_looping_method(node) {
                return;
            }
            let Some(sibling) = sibling.filter(|s| same_collection_looping_block(node, *s)) else {
                return;
            };
            if node.body().is_none() || sibling.body().is_none() {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                if !same_child(block_arguments(node), block_arguments(sibling)) {
                    return;
                }
                combine_with_left_sibling(corrector, node, sibling);
            });
        }
    }
}

/// `BlockNode#arguments`: the `args` node (an empty array for numbered and
/// `it` blocks, so they compare equal to each other).
fn block_arguments(node: Node<'_>) -> Option<Node<'_>> {
    if node.is(NodeType::Block) {
        node.child(1)
    } else {
        None
    }
}

fn same_child(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
    }
}

fn collection_looping_method(node: Node<'_>) -> bool {
    let method_name = node.method_name().unwrap_or("");
    method_name.starts_with("each") || method_name.ends_with("_each")
}

fn same_collection_looping_block(node: Node<'_>, sibling: Node<'_>) -> bool {
    if !sibling.is_any_block() {
        return false;
    }
    let (send, sibling_send) = (node.send_node().unwrap(), sibling.send_node().unwrap());
    let arguments = send.arguments();
    let sibling_arguments = sibling_send.arguments();
    sibling.method_name() == node.method_name()
        && same_child(sibling.receiver(), node.receiver())
        && arguments.len() == sibling_arguments.len()
        && arguments
            .iter()
            .zip(&sibling_arguments)
            .all(|(a, b)| a.same_as(*b))
}

fn same_collection_looping_for(node: Node<'_>, sibling: Node<'_>) -> bool {
    sibling.is(NodeType::For) && same_child(node.child(1), sibling.child(1))
}

fn combine_with_left_sibling(corrector: &mut Corrector, node: Node<'_>, sibling: Node<'_>) {
    corrector.remove(
        sibling
            .body()
            .unwrap()
            .source_range()
            .end()
            .join(sibling.loc().end.unwrap()),
    );
    corrector.remove(
        node.source_range()
            .begin()
            .join(node.body().unwrap().source_range().begin()),
    );
    correct_end_of_block(corrector, node, sibling);
}

fn correct_end_of_block(corrector: &mut Corrector, node: Node<'_>, sibling: Node<'_>) {
    if !sibling.is_any_block() {
        return;
    }
    if node.right_sibling().is_some_and(|s| s.is_any_block()) {
        return;
    }
    let end_of_block = if sibling.has_braces() { "}" } else { " end" };
    corrector.remove(node.loc().end.unwrap());
    corrector.insert_before(node.source_range().end(), end_of_block);
}
