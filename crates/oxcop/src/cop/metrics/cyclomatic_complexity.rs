//! Port of `lib/rubocop/cop/metrics/cyclomatic_complexity.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::method_complexity::{
    COUNTED_NODES, Complexity, NODE_TYPES, check_method_complexity, complexity,
    complexity_score_for, method_to_check,
};
use crate::cop::{Cop, Investigation};


#[derive(Default)]
pub struct CyclomaticComplexity;

impl Cop for CyclomaticComplexity {
    fn name(&self) -> &'static str {
        "Metrics/CyclomaticComplexity"
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
                    "Cyclomatic complexity for `{method}` is too high. [{}/{}]",
                    complexity.score as i64, max as i64
                )
            },
        );
    }
}

