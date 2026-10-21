//! Port of `lib/rubocop/cop/mixin/string_help.rb`. A cop keeps a
//! `StringHelp` field, lists [`NODE_TYPES`] in `node_types`, and in
//! `on_node` asks [`StringHelp::candidate`] for the `str` node to check;
//! `if offense?(node)` and the `add_offense` stay in the cop.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;

/// `on_str` and `on_regexp`.
pub const NODE_TYPES: &[NodeType] = &[NodeType::Str, NodeType::Regexp];

#[derive(Debug, Default)]
pub struct StringHelp {
    pub ignored: IgnoredNodes,
}

impl StringHelp {
    /// The guards of `on_str`, and `on_regexp` (which ignores the regexp).
    /// Returns the node when the cop should run `offense?` on it.
    pub fn candidate<'a>(&mut self, node: Node<'a>) -> Option<Node<'a>> {
        if node.is(NodeType::Regexp) {
            self.ignored.ignore(node);
            return None;
        }
        if node.loc().begin.is_none() || self.ignored.part_of(node) {
            return None;
        }
        Some(node)
    }
}

/// `inside_interpolation?(node)`.
pub fn inside_interpolation(node: Node<'_>) -> bool {
    node.ancestors()
        .skip_while(|a| !a.is(NodeType::Begin))
        .any(|a| matches!(a.ty(), NodeType::Dstr | NodeType::Dsym | NodeType::Regexp))
}
