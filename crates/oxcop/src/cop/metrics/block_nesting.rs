//! Port of `lib/rubocop/cop/metrics/block_nesting.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const NESTING_BLOCKS: &[NodeType] = &[
    NodeType::Case,
    NodeType::CaseMatch,
    NodeType::If,
    NodeType::While,
    NodeType::WhilePost,
    NodeType::Until,
    NodeType::UntilPost,
    NodeType::For,
    NodeType::Resbody,
];

#[derive(Default)]
pub struct BlockNesting {
    ignored: IgnoredNodes,
}

impl Cop for BlockNesting {
    fn name(&self) -> &'static str {
        "Metrics/BlockNesting"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        let max = cx.cop_config_int("Max").unwrap_or(0);
        self.check_nesting_level(cx, ast, max, 0);
    }
}

impl BlockNesting {
    fn check_nesting_level<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        max: i64,
        mut current_level: i64,
    ) {
        if consider_node(cx, node) {
            if count_if_block(cx, node) {
                current_level += 1;
            }
            if current_level > max {
                cx.set_exclude_limit("Max", current_level);
                if !self.ignored.part_of(node) {
                    let message = format!("Avoid more than {max} levels of block nesting.");
                    let ignored = &mut self.ignored;
                    cx.add_offense_cx(node.source_range(), &message, |_, _| ignored.ignore(node));
                }
            }
        }
        for child_node in node.child_nodes() {
            self.check_nesting_level(cx, child_node, max, current_level);
        }
    }
}

fn count_if_block(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !node.is(NodeType::If) {
        return true;
    }
    if node.is_elsif() {
        return false;
    }
    if node.is_modifier_form() {
        return cx.cop_config_bool("CountModifierForms", false);
    }
    true
}

fn consider_node(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if NESTING_BLOCKS.contains(&node.ty()) {
        return true;
    }
    cx.cop_config_bool("CountBlocks", false) && node.is_any_block()
}
