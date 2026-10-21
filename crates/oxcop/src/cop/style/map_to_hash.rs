//! Port of `lib/rubocop/cop/style/map_to_hash.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::{Cop, Corrector, Investigation};

static MAP_TO_H: NodePattern = NodePattern::new(
    "{
      $(call (any_block $(call _ {:map :collect}) ...) :to_h)
      $(call $(call _ {:map :collect} (block_pass sym)) :to_h)
    }",
);
static DESTRUCTURING_ARGUMENT: NodePattern = NodePattern::new("(args $(mlhs (arg _)+))");

#[derive(Default)]
pub struct MapToHash;

impl Cop for MapToHash {
    fn name(&self) -> &'static str {
        "Style/MapToHash"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Layout/SingleLineBlockChain"]
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["to_h"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = MAP_TO_H.match_node(node) else {
            return;
        };
        let to_h_node = captures[0].node().unwrap();
        let map_node = captures[1].node().unwrap();
        if to_h_node.is_block_literal() {
            return;
        }
        let selector = map_node.loc().selector.unwrap();
        let message = format!(
            "Pass a block to `to_h` instead of calling `{}{}to_h`.",
            map_node.range_source(selector),
            to_h_node.range_source(to_h_node.loc().dot.unwrap())
        );
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(selector, &message, |corrector| {
            autocorrect(src, corrector, to_h_node, map_node);
        });
    }
}

fn autocorrect(src: &str, corrector: &mut Corrector, to_h: Node<'_>, map: Node<'_>) {
    let to_h_dot = to_h.loc().dot.unwrap();
    let removal_range = range_between(to_h_dot.begin_pos, to_h.loc().selector.unwrap().end_pos);
    corrector.remove(range_with_surrounding_space(
        src,
        removal_range,
        SurroundingSpace::side(Side::Left),
    ));
    if let Some(map_dot) = map.loc().dot {
        corrector.replace(map_dot, to_h.range_source(to_h_dot));
    }
    corrector.replace(map.loc().selector.unwrap(), "to_h");

    let Some(parent) = map.parent().filter(|p| p.is(NodeType::Block)) else {
        return;
    };
    if let Some(argument) = parent
        .child(1)
        .and_then(|args| DESTRUCTURING_ARGUMENT.match_node(args))
        .and_then(|c| c[0].node())
    {
        let source = argument.source();
        corrector.replace(argument.source_range(), &source[1..source.len() - 1]);
    }
}
