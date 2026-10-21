//! Port of `lib/rubocop/cop/style/collection_compact.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_receivers::allowed_receiver;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const TO_ENUM_METHODS: &[&str] = &["to_enum", "lazy"];
const FILTER_METHODS: &[&str] = &["filter", "filter!"];

static REJECT_METHOD_WITH_BLOCK_PASS: NodePattern = NodePattern::new(
    "(call !nil? {:reject :reject!}
       (block_pass
         (sym :nil?)))",
);

static REJECT_METHOD: NodePattern = NodePattern::new(
    "(block
       (call !nil? {:reject :reject!})
       $(args ...)
       (call $(lvar _) :nil?))",
);

static REJECT_METHOD_FOR_NUMBLOCK_OR_ITBLOCK: NodePattern = NodePattern::new(
    "{
       (numblock (call !nil? {:reject :reject!}) _ (call (lvar :_1) :nil?))
       (itblock (call !nil? {:reject :reject!}) _ (call (lvar :it) :nil?))
     }",
);

static SELECT_METHOD: NodePattern = NodePattern::new(
    "(block
       (call !nil? {:select :select! :filter :filter!})
       $(args ...)
       (call
         (call $(lvar _) :nil?) :!))",
);

static SELECT_METHOD_FOR_NUMBLOCK_OR_ITBLOCK: NodePattern = NodePattern::new(
    "{
       (numblock (call !nil? {:select :select! :filter :filter!}) _ (call (call (lvar :_1) :nil?) :!))
       (itblock (call !nil? {:select :select! :filter :filter!}) _ (call (call (lvar :it) :nil?) :!))
     }",
);

static GREP_V_WITH_NIL: NodePattern =
    NodePattern::new("(call _ :grep_v {(nil) (const {nil? cbase} :NilClass)})");

#[derive(Default)]
pub struct CollectionCompact;

impl Cop for CollectionCompact {
    fn name(&self) -> &'static str {
        "Style/CollectionCompact"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.4)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "reject", "reject!", "select", "select!", "filter", "filter!", "grep_v",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method_name = node.method_name().unwrap_or("");
        if cx.target_ruby_version() < 2.6 && FILTER_METHODS.contains(&method_name) {
            return;
        }
        let Some(range) = offense_range(node) else {
            return;
        };
        if node.receiver().is_some_and(|r| allowed_receiver(cx, r)) {
            return;
        }
        if cx.target_ruby_version() <= 3.0 && to_enum_method(node) {
            return;
        }
        let good = if node.is_bang_method() {
            "compact!"
        } else {
            "compact"
        };
        let message = format!("Use `{good}` instead of `{}`.", cx.source(range));
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, good);
        });
    }
}

fn offense_range(node: Node<'_>) -> Option<Range> {
    if REJECT_METHOD_WITH_BLOCK_PASS.is_match(node) || GREP_V_WITH_NIL.is_match(node) {
        return Some(range(node, node));
    }
    let block_node = node.parent()?;
    if !block_node.is_any_block() || !match_block_method(block_node) {
        return None;
    }
    Some(range(node, block_node))
}

fn match_block_method(block_node: Node<'_>) -> bool {
    if block_node.is(NodeType::Block) {
        let Some(captures) = REJECT_METHOD
            .match_node(block_node)
            .or_else(|| SELECT_METHOD.match_node(block_node))
        else {
            return false;
        };
        let args = captures[0].node().unwrap();
        let receiver = captures[1].node().unwrap();
        args.child_nodes()
            .last()
            .is_some_and(|last| last.source() == receiver.source())
    } else {
        REJECT_METHOD_FOR_NUMBLOCK_OR_ITBLOCK.is_match(block_node)
            || SELECT_METHOD_FOR_NUMBLOCK_OR_ITBLOCK.is_match(block_node)
    }
}

fn to_enum_method(node: Node<'_>) -> bool {
    let Some(receiver) = node.receiver().filter(|r| r.is_call()) else {
        return false;
    };
    TO_ENUM_METHODS.contains(&receiver.method_name().unwrap_or(""))
}

fn range(begin_pos_node: Node<'_>, end_pos_node: Node<'_>) -> Range {
    range_between(
        begin_pos_node.loc().selector.unwrap().begin_pos,
        end_pos_node.source_range().end_pos,
    )
}
