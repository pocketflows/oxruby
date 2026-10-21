//! Port of `lib/rubocop/cop/style/redundant_struct_keyword_init.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static STRUCT_NEW: NodePattern = NodePattern::new("(call (const {nil? cbase} :Struct) :new ...)");
static REDUNDANT_KEYWORD_INIT: NodePattern =
    NodePattern::new("(pair (sym :keyword_init) {(true) (nil)})");
static KEYWORD_INIT_FALSE: NodePattern = NodePattern::new("(pair (sym :keyword_init) (false))");

fn keyword_init(pair: Node<'_>) -> bool {
    REDUNDANT_KEYWORD_INIT.is_match(pair) || KEYWORD_INIT_FALSE.is_match(pair)
}

#[derive(Default)]
pub struct RedundantStructKeywordInit;

impl Cop for RedundantStructKeywordInit {
    fn name(&self) -> &'static str {
        "Style/RedundantStructKeywordInit"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.2)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !STRUCT_NEW.is_match(node) {
            return;
        }
        let Some(last) = node.last_argument().filter(|a| a.is(NodeType::Hash)) else {
            return;
        };
        let keyword_init_nodes: Vec<Node<'a>> =
            last.pairs().into_iter().filter(|p| keyword_init(*p)).collect();
        if keyword_init_nodes.iter().any(|n| KEYWORD_INIT_FALSE.is_match(*n)) {
            return;
        }
        for keyword_init in keyword_init_nodes {
            if !REDUNDANT_KEYWORD_INIT.is_match(keyword_init) {
                continue;
            }
            let message = format!(
                "Remove the redundant `keyword_init: {}`.",
                keyword_init.pair_value().unwrap().source()
            );
            cx.add_offense_with_correction(keyword_init.source_range(), &message, |corrector| {
                corrector.remove(range(keyword_init));
            });
        }
    }
}

fn range(redundant_keyword_init: Node<'_>) -> Range {
    let hash = redundant_keyword_init.parent().unwrap();
    if hash.pairs().into_iter().all(keyword_init) {
        range_emptying_hash(redundant_keyword_init, hash)
    } else {
        range_within_hash(redundant_keyword_init)
    }
}

fn range_within_hash(pair: Node<'_>) -> Range {
    if let Some(left) = pair.left_sibling() {
        left.source_range().end().join(pair.source_range().end())
    } else if let Some(right) = pair.right_sibling() {
        pair.source_range().begin().join(right.source_range().begin())
    } else {
        pair.source_range()
    }
}

fn range_emptying_hash(pair: Node<'_>, hash: Node<'_>) -> Range {
    match hash.left_sibling() {
        Some(preceding) => preceding.source_range().end().join(pair.source_range().end()),
        None => pair.source_range(),
    }
}
