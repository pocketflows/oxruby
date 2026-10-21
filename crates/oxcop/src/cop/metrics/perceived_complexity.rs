//! Port of `lib/rubocop/cop/metrics/perceived_complexity.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::metrics::utils::repeated_csend_discount::RepeatedCsendDiscount;
use crate::cop::mixin::method_complexity::{
    Complexity, NODE_TYPES, check_method_complexity, complexity,
    complexity_score_for as cyclomatic_score, method_to_check,
};
use crate::cop::{Cop, Investigation};

/// `CyclomaticComplexity::COUNTED_NODES - %i[when in_pattern] + %i[case case_match]`.
const COUNTED_NODES: &[NodeType] = &[
    NodeType::If,
    NodeType::While,
    NodeType::Until,
    NodeType::For,
    NodeType::Csend,
    NodeType::Block,
    NodeType::BlockPass,
    NodeType::Rescue,
    NodeType::And,
    NodeType::Or,
    NodeType::OrAsgn,
    NodeType::AndAsgn,
    NodeType::Case,
    NodeType::CaseMatch,
];

#[derive(Default)]
pub struct PerceivedComplexity;

impl Cop for PerceivedComplexity {
    fn name(&self) -> &'static str {
        "Metrics/PerceivedComplexity"
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(method_name) = method_to_check(cx, node) else {
            return;
        };
        check_method_complexity(
            cx,
            node,
            method_name,
            |_, body| Complexity {
                score: complexity(body, COUNTED_NODES, complexity_score_for) as f64,
                abc_vector: None,
            },
            |method, complexity, max| {
                format!(
                    "Perceived complexity for `{method}` is too high. [{}/{}]",
                    complexity.score as i64, max as i64
                )
            },
        );
    }
}

fn complexity_score_for(node: Node<'_>, csend: &mut RepeatedCsendDiscount) -> usize {
    match node.ty() {
        NodeType::Case => {
            // If cond is nil, that means each when has an expression that
            // evaluates to true or false. It's just an alternative to
            // if/elsif/elsif... so the when nodes count.
            let nb_branches =
                node.when_branches().len() + usize::from(node.else_branch().is_some());
            if node.child(0).is_none() {
                nb_branches
            } else {
                // Otherwise, the case node gets 0.8 complexity points and
                // each when gets 0.2.
                ((nb_branches as f64 * 0.2) + 0.8).round() as usize
            }
        }
        NodeType::CaseMatch => {
            // Simple `in` branches are discounted like `when`, while
            // structural patterns keep the full complexity of a decision
            // point.
            let mut score: f64 = node
                .when_branches()
                .iter()
                .map(|branch| if simple_in_pattern(*branch) { 0.2 } else { 1.0 })
                .sum();
            if node.else_branch().is_some() {
                score += 0.2;
            }
            score.round() as usize
        }
        NodeType::If => {
            if node.has_else() && !node.is_elsif() {
                2
            } else {
                1
            }
        }
        _ => cyclomatic_score(node, csend),
    }
}

fn simple_in_pattern(in_pattern_node: Node<'_>) -> bool {
    // `in_pattern_node.children[1]` is the guard (`if`/`unless`), or `nil`.
    if !in_pattern_node.children().get(1).is_some_and(|c| c.is_nil()) {
        return false;
    }
    // A scalar literal, a literal range, or a constant/type is as easy to
    // read as a `when`.
    in_pattern_node
        .child(0)
        .is_some_and(|pattern| pattern.is_literal() || pattern.is(NodeType::Const))
}
