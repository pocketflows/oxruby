//! Port of `lib/rubocop/cop/style/redundant_sort_by.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static REDUNDANT_SORT_BY_BLOCK: NodePattern =
    NodePattern::new("(block $(call _ :sort_by) (args (arg $_x)) (lvar _x))");
static REDUNDANT_SORT_BY_NUMBLOCK: NodePattern =
    NodePattern::new("(numblock $(call _ :sort_by) 1 (lvar :_1))");
static REDUNDANT_SORT_BY_ITBLOCK: NodePattern =
    NodePattern::new("(itblock $(call _ :sort_by) _ (lvar :it))");

#[derive(Default)]
pub struct RedundantSortBy;

impl Cop for RedundantSortBy {
    fn name(&self) -> &'static str {
        "Style/RedundantSortBy"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let (send, message) = match node.ty() {
            NodeType::Block => {
                let Some(c) = REDUNDANT_SORT_BY_BLOCK.match_node(node) else {
                    return;
                };
                let var = c[1].sym().unwrap_or("");
                (
                    c[0].node().unwrap(),
                    format!("Use `sort` instead of `sort_by {{ |{var}| {var} }}`."),
                )
            }
            NodeType::Numblock => {
                let Some(c) = REDUNDANT_SORT_BY_NUMBLOCK.match_node(node) else {
                    return;
                };
                (
                    c[0].node().unwrap(),
                    "Use `sort` instead of `sort_by { _1 }`.".to_owned(),
                )
            }
            _ => {
                let Some(c) = REDUNDANT_SORT_BY_ITBLOCK.match_node(node) else {
                    return;
                };
                (
                    c[0].node().unwrap(),
                    "Use `sort` instead of `sort_by { it }`.".to_owned(),
                )
            }
        };
        let range = range_between(
            send.loc().selector.unwrap().begin_pos,
            node.loc().end.unwrap().end_pos,
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, "sort");
        });
    }
}
