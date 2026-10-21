//! Port of `lib/rubocop/cop/style/hash_like_case.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::min_branches_count::min_branches_count;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Consider replacing `case-when` with a hash lookup.";

static HASH_LIKE_CASE: NodePattern = NodePattern::new(
    "(case
      _
      (when
        ${str_type? sym_type?}
        $[!nil? recursive_basic_literal?])+ nil?)",
);

#[derive(Default)]
pub struct HashLikeCase;

impl Cop for HashLikeCase {
    fn name(&self) -> &'static str {
        "Style/HashLikeCase"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !min_branches_count(cx, node) {
            return;
        }
        let Some(captures) = HASH_LIKE_CASE.match_node(node) else {
            return;
        };
        let condition_nodes = captures[0].nodes();
        let body_nodes = captures[1].nodes();
        if nodes_of_same_type(&condition_nodes) && nodes_of_same_type(&body_nodes) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}

fn nodes_of_same_type(nodes: &[Node<'_>]) -> bool {
    nodes.iter().all(|n| Some(n.ty()) == nodes.first().map(|f| f.ty()))
}
