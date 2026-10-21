//! Port of `lib/rubocop/cop/style/map_to_set.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};

static MAP_TO_SET: NodePattern = NodePattern::new(
    "{
      $(call (any_block $(call _ {:map :collect}) ...) :to_set)
      $(call $(call _ {:map :collect} (block_pass sym)) :to_set)
    }",
);

#[derive(Default)]
pub struct MapToSet;

impl Cop for MapToSet {
    fn name(&self) -> &'static str {
        "Style/MapToSet"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["to_set"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = MAP_TO_SET.match_node(node) else {
            return;
        };
        let to_set = captures[0].node().unwrap();
        let map = captures[1].node().unwrap();
        if to_set.is_block_literal() {
            return;
        }
        let selector = map.loc().selector.unwrap();
        let message = format!(
            "Pass a block to `to_set` instead of calling `{}.to_set`.",
            map.range_source(selector)
        );
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(selector, &message, |corrector| {
            let removal_range = range_between(
                to_set.loc().dot.unwrap().begin_pos,
                to_set.loc().selector.unwrap().end_pos,
            );
            corrector.remove(range_with_surrounding_space(
                src,
                removal_range,
                SurroundingSpace::side(Side::Left),
            ));
            corrector.replace(selector, "to_set");
        });
    }
}
