//! Port of `lib/rubocop/cop/style/comparable_between.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static LOGICAL_COMPARISON_BETWEEN_BY_MIN_FIRST: NodePattern = NodePattern::new(
    "(and
       (send
         {$_value :>= $_min | $_min :<= $_value})
       (send
         {$_value :<= $_max | $_max :>= $_value}))",
);

static LOGICAL_COMPARISON_BETWEEN_BY_MAX_FIRST: NodePattern = NodePattern::new(
    "(and
       (send
         {$_value :<= $_max | $_max :>= $_value})
       (send
         {$_value :>= $_min | $_min :<= $_value}))",
);

#[derive(Default)]
pub struct ComparableBetween;

impl Cop for ComparableBetween {
    fn name(&self) -> &'static str {
        "Style/ComparableBetween"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::And]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(captures) = LOGICAL_COMPARISON_BETWEEN_BY_MIN_FIRST.match_node(node) {
            let args: Vec<Node<'_>> = captures.iter().filter_map(|c| c.node()).collect();
            register_offense(cx, node, &args[0..2], &args[2..4]);
        }
        if let Some(captures) = LOGICAL_COMPARISON_BETWEEN_BY_MAX_FIRST.match_node(node) {
            let args: Vec<Node<'_>> = captures.iter().filter_map(|c| c.node()).collect();
            register_offense(cx, node, &args[2..4], &args[0..2]);
        }
    }
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    min_and_value: &[Node<'_>],
    max_and_value: &[Node<'_>],
) {
    if min_and_value.len() != 2 || max_and_value.len() != 2 {
        return;
    }
    // `(min_and_value & max_and_value).first`
    let Some(value) = min_and_value
        .iter()
        .find(|a| max_and_value.iter().any(|b| a.same_as(*b)))
        .copied()
    else {
        return;
    };
    let min = min_and_value
        .iter()
        .find(|n| !n.same_as(value))
        .copied()
        .unwrap_or(value);
    let max = max_and_value
        .iter()
        .find(|n| !n.same_as(value))
        .copied()
        .unwrap_or(value);
    let prefer = format!(
        "{}.between?({}, {})",
        value.source(),
        min.source(),
        max.source()
    );
    let message = format!("Prefer `{prefer}` over logical comparison.");
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        corrector.replace(node.source_range(), prefer);
    });
}
