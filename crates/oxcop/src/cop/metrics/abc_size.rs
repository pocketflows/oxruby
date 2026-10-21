//! Port of `lib/rubocop/cop/metrics/abc_size.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::metrics::utils::abc_size_calculator::AbcSizeCalculator;
use crate::cop::mixin::method_complexity::{
    Complexity, NODE_TYPES, check_method_complexity, format_g4, method_to_check,
};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct AbcSize;

impl Cop for AbcSize {
    fn name(&self) -> &'static str {
        "Metrics/AbcSize"
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
            |cx, body| {
                let discount = !cx.cop_config_bool("CountRepeatedAttributes", false);
                let (score, vector) = AbcSizeCalculator::calculate(body, discount);
                Complexity {
                    score,
                    abc_vector: Some(vector),
                }
            },
            |method, complexity, max| {
                format!(
                    "Assignment Branch Condition size for `{method}` is too high. [{} {}/{}]",
                    complexity.abc_vector.as_deref().unwrap_or(""),
                    format_g4(complexity.score),
                    format_g4(max)
                )
            },
        );
    }
}
