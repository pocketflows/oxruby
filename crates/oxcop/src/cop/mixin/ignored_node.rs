//! Port of `lib/rubocop/cop/ignored_node.rb`. Cops keep an `IgnoredNodes`
//! field (the Ruby mixin keeps `@ignored_nodes`).

use crate::ast::{MapKind, Node, NodeId};

#[derive(Debug, Default)]
pub struct IgnoredNodes {
    nodes: Vec<(usize, usize, NodeId)>,
}

impl IgnoredNodes {
    /// `ignore_node(node)`.
    pub fn ignore(&mut self, node: Node<'_>) {
        let begin = node.source_range().begin_pos;
        let end = if node.loc().kind == MapKind::Heredoc {
            node.loc()
                .heredoc_end
                .map_or(node.source_range().end_pos, |r| r.end_pos)
        } else {
            node.source_range().end_pos
        };
        self.nodes.push((begin, end, node.id()));
    }

    /// `part_of_ignored_node?(node)`.
    pub fn part_of(&self, node: Node<'_>) -> bool {
        let range = node.source_range();
        self.nodes
            .iter()
            .any(|&(begin, end, _)| begin <= range.begin_pos && end >= range.end_pos)
    }

    /// `ignored_node?(node)`.
    pub fn contains(&self, node: Node<'_>) -> bool {
        self.nodes.iter().any(|&(_, _, id)| id == node.id())
    }
}
