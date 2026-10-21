//! Port of `lib/rubocop/cop/lint/require_range_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct RequireRangeParentheses;

impl Cop for RequireRangeParentheses {
    fn name(&self) -> &'static str {
        "Lint/RequireRangeParentheses"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Irange, NodeType::Erange]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.parent().is_some_and(|p| p.is(NodeType::Begin)) {
            return;
        }
        let (Some(begin), Some(end)) = (node.child(0), node.child(1)) else {
            return;
        };
        let Some(operator) = node.loc().operator else {
            return;
        };
        let ps = cx.processed_source;
        if ps.line_of(operator.begin_pos) == ps.line_of(end.source_range().begin_pos) {
            return;
        }

        let message = format!(
            "Wrap the range literal `{}{}` in parentheses to avoid confusion with an endless range.",
            begin.source(),
            node.range_source(operator)
        );
        cx.add_offense(node.source_range(), &message);
    }
}
