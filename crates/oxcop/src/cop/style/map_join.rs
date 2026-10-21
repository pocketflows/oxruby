//! Port of `lib/rubocop/cop/style/map_join.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

// map(&:to_s).join(...)
static MAP_TO_S_JOIN: NodePattern = NodePattern::new(
    "(call
      $(call _ ${:map :collect} (block_pass (sym :to_s)))
      :join ...)",
);
// map { |x| x.to_s }.join(...)
static MAP_TO_S_BLOCK_JOIN: NodePattern = NodePattern::new(
    "(call
      $(block
        (call _ ${:map :collect})
        (args (arg _x))
        (send (lvar _x) :to_s))
      :join ...)",
);
// map { _1.to_s }.join(...)
static MAP_TO_S_NUMBLOCK_JOIN: NodePattern = NodePattern::new(
    "(call
      $(numblock
        (call _ ${:map :collect})
        1
        (send (lvar :_1) :to_s))
      :join ...)",
);
// map { it.to_s }.join(...)
static MAP_TO_S_ITBLOCK_JOIN: NodePattern = NodePattern::new(
    "(call
      $(itblock
        (call _ ${:map :collect})
        :it
        (send (lvar :it) :to_s))
      :join ...)",
);

#[derive(Default)]
pub struct MapJoin;

impl Cop for MapJoin {
    fn name(&self) -> &'static str {
        "Style/MapJoin"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["join"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let captures = [
            &MAP_TO_S_JOIN,
            &MAP_TO_S_BLOCK_JOIN,
            &MAP_TO_S_NUMBLOCK_JOIN,
            &MAP_TO_S_ITBLOCK_JOIN,
        ]
        .iter()
        .find_map(|p| p.match_node(node));
        let Some(captures) = captures else {
            return;
        };
        let map_node = captures[0].node().unwrap();
        let method_name = captures[1].sym().unwrap_or("");
        register_offense(cx, node, map_node, method_name);
    }
}

fn register_offense(cx: &mut Investigation<'_, '_>, join_node: Node<'_>, map_node: Node<'_>, method_name: &str) {
    let map_send = if map_node.is_any_block() {
        map_node.send_node().unwrap()
    } else {
        map_node
    };
    let message = format!("Remove redundant `{method_name}(&:to_s)` before `join`.");
    let ps = cx.processed_source;
    cx.add_offense_with_correction(map_send.loc().selector.unwrap(), &message, |corrector| {
        let range = match map_send.receiver() {
            Some(receiver) => {
                let dot = map_send.loc().dot.unwrap();
                let start_pos = if receiver.last_line() < ps.line_of(dot.begin_pos) {
                    receiver.source_range().end_pos
                } else {
                    dot.begin_pos
                };
                range_between(start_pos, map_node.source_range().end_pos)
            }
            None => no_receiver_range(map_node, join_node),
        };
        corrector.replace(range, "");
    });
}

fn no_receiver_range(map_node: Node<'_>, join_node: Node<'_>) -> Range {
    range_between(
        map_node.source_range().begin_pos,
        join_node.loc().dot.unwrap().end_pos,
    )
}
