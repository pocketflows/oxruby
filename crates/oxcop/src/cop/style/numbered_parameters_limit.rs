//! Port of `lib/rubocop/cop/style/numbered_parameters_limit.rb`.

use std::collections::HashSet;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const DEFAULT_MAX_VALUE: i64 = 1;

#[derive(Default)]
pub struct NumberedParametersLimit;

impl Cop for NumberedParametersLimit {
    fn name(&self) -> &'static str {
        "Style/NumberedParametersLimit"
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Numblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let param_count = numbered_parameter_nodes(node).len() as i64;
        let max_count = max_count(cx);
        if param_count <= max_count {
            return;
        }
        let parameter = if max_count > 1 { "parameters" } else { "parameter" };
        let message = format!(
            "Avoid using more than {max_count} numbered {parameter}; {param_count} detected."
        );
        let range = node.source_range();
        // `add_offense(...) { self.max = param_count }`
        if !cx.has_offense_at(range) && cx.enabled_line(cx.processed_source.line_of(range.begin_pos)) {
            cx.set_exclude_limit("Max", param_count);
        }
        cx.add_offense(range, &message);
    }
}

/// The distinct numbered parameters (`uniq` on structurally equal nodes).
fn numbered_parameter_nodes<'a>(node: Node<'a>) -> HashSet<&'a str> {
    node.descendants()
        .filter(|d| d.is(NodeType::Lvar))
        .map(|d| d.source())
        .filter(|s| {
            let b = s.as_bytes();
            b.len() == 2 && b[0] == b'_' && (b'1'..=b'9').contains(&b[1])
        })
        .collect()
}

fn max_count(cx: &Investigation<'_, '_>) -> i64 {
    let max = cx.cop_config_int("Max").unwrap_or(DEFAULT_MAX_VALUE);
    max.min(9)
}
