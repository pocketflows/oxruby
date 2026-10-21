//! Port of `lib/rubocop/cop/style/tally_method.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_EACH_WITH_OBJECT: &str = "Use `tally` instead of `each_with_object`.";
const MSG_GROUP_BY: &str = "Use `tally` instead of `group_by` and `transform_values`.";

static TALLY_EACH_WITH_OBJECT: NodePattern = NodePattern::new(
    "{
       (block
         (call _ :each_with_object
           (send (const {nil? cbase} :Hash) :new (int 0)))
         (args (arg _elem) (arg _hash))
         (op_asgn
           (send (lvar _hash) :[] (lvar _elem)) :+ (int 1)))
       (numblock
         (call _ :each_with_object
           (send (const {nil? cbase} :Hash) :new (int 0)))
         2
         (op_asgn
           (send (lvar :_2) :[] (lvar :_1)) :+ (int 1)))
     }",
);

// `%COUNTING_METHODS` is inlined as `{:count :size :length}`.
static TALLY_GROUP_BY_SYMBOL: NodePattern = NodePattern::new(
    "(call
       (call _ :group_by (block_pass (sym :itself)))
       :transform_values
       (block_pass (sym {:count :size :length})))",
);

static TALLY_GROUP_BY_IDENTITY_BLOCK: NodePattern = NodePattern::new(
    "(call
       {
         (block (call _ :group_by) (args (arg _x)) (lvar _x))
         (numblock (call _ :group_by) 1 (lvar :_1))
         (itblock (call _ :group_by) :it (lvar :it))
       }
       :transform_values
       (block_pass (sym {:count :size :length})))",
);

static TALLY_GROUP_BY_TRANSFORM_BLOCK: NodePattern = NodePattern::new(
    "{
       (block
         (call
           {
             (call _ :group_by (block_pass (sym :itself)))
             (block (call _ :group_by) (args (arg _x)) (lvar _x))
             (numblock (call _ :group_by) 1 (lvar :_1))
             (itblock (call _ :group_by) :it (lvar :it))
           }
           :transform_values)
         (args (arg _v))
         (send (lvar _v) {:count :size :length}))
       (numblock
         (call
           {
             (call _ :group_by (block_pass (sym :itself)))
             (block (call _ :group_by) (args (arg _x)) (lvar _x))
             (numblock (call _ :group_by) 1 (lvar :_1))
             (itblock (call _ :group_by) :it (lvar :it))
           }
           :transform_values)
         1
         (send (lvar :_1) {:count :size :length}))
       (itblock
         (call
           {
             (call _ :group_by (block_pass (sym :itself)))
             (block (call _ :group_by) (args (arg _x)) (lvar _x))
             (numblock (call _ :group_by) 1 (lvar :_1))
             (itblock (call _ :group_by) :it (lvar :it))
           }
           :transform_values)
         :it
         (send (lvar :it) {:count :size :length}))
     }",
);

#[derive(Default)]
pub struct TallyMethod;

impl Cop for TallyMethod {
    fn name(&self) -> &'static str {
        "Style/TallyMethod"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["each_with_object", "transform_values"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_method("each_with_object") {
            check_each_with_object(cx, node);
        } else if node.is_method("transform_values") {
            check_transform_values(cx, node);
        }
    }
}

fn check_each_with_object(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(block_node) = node.block_node() else {
        return;
    };
    if !TALLY_EACH_WITH_OBJECT.is_match(block_node) {
        return;
    }
    let range = replacement_range(node, block_node);
    cx.add_offense_with_correction(node.loc().selector.unwrap(), MSG_EACH_WITH_OBJECT, |c| {
        c.replace(range, "tally");
    });
}

fn check_transform_values(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if TALLY_GROUP_BY_SYMBOL.is_match(node) || TALLY_GROUP_BY_IDENTITY_BLOCK.is_match(node) {
        register_group_by_offense(cx, node, node);
    } else if let Some(block_node) = node.block_node()
        && TALLY_GROUP_BY_TRANSFORM_BLOCK.is_match(block_node)
    {
        register_group_by_offense(cx, node, block_node);
    }
}

fn register_group_by_offense(
    cx: &mut Investigation<'_, '_>,
    transform_node: Node<'_>,
    end_node: Node<'_>,
) {
    let receiver = transform_node.receiver().unwrap();
    let group_by_node = if receiver.is_any_block() {
        receiver.send_node().unwrap()
    } else {
        receiver
    };
    let range = replacement_range(group_by_node, end_node);
    cx.add_offense_with_correction(group_by_node.loc().selector.unwrap(), MSG_GROUP_BY, |c| {
        c.replace(range, "tally");
    });
}

fn replacement_range(start_node: Node<'_>, end_node: Node<'_>) -> Range {
    range_between(
        start_node.loc().selector.unwrap().begin_pos,
        end_node.source_range().end_pos,
    )
}
