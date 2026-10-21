//! Port of `lib/rubocop/cop/style/unless_logical_operators.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const FORBID_MIXED_LOGICAL_OPERATORS: &str = "Do not use mixed logical operators in an `unless`.";
const FORBID_LOGICAL_OPERATORS: &str = "Do not use any logical operator in an `unless`.";

static OR_WITH_AND: NodePattern = NodePattern::new("(if (or <`and ...> ) ...)");
static AND_WITH_OR: NodePattern = NodePattern::new("(if (and <`or ...> ) ...)");
static LOGICAL_OPERATOR: NodePattern = NodePattern::new("(if ({and or} ... ) ...)");

#[derive(Default)]
pub struct UnlessLogicalOperators;

impl Cop for UnlessLogicalOperators {
    fn name(&self) -> &'static str {
        "Style/UnlessLogicalOperators"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_unless() {
            return;
        }
        let style = style(cx);
        if style == "forbid_mixed_logical_operators" && mixed_logical_operator(node) {
            cx.add_offense(node.source_range(), FORBID_MIXED_LOGICAL_OPERATORS);
        } else if style == "forbid_logical_operators" && LOGICAL_OPERATOR.is_match(node) {
            cx.add_offense(node.source_range(), FORBID_LOGICAL_OPERATORS);
        }
    }
}

fn mixed_logical_operator(node: Node<'_>) -> bool {
    OR_WITH_AND.is_match(node)
        || AND_WITH_OR.is_match(node)
        || mixed_precedence(node, NodeType::And, "&&", "and")
        || mixed_precedence(node, NodeType::Or, "||", "or")
}

fn mixed_precedence(node: Node<'_>, ty: NodeType, symbolic: &str, word: &str) -> bool {
    let condition = node.condition().unwrap();
    let mut sources: Vec<&str> = condition
        .descendants()
        .filter(|d| d.is(ty))
        .filter_map(|d| d.operator())
        .collect();
    if condition.is(ty) {
        sources.extend(condition.operator());
    }
    !(sources.iter().all(|s| *s == symbolic) || sources.iter().all(|s| *s == word))
}
