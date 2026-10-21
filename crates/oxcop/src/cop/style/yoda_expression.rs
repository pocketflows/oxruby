//! Port of `lib/rubocop/cop/style/yoda_expression.rb`.

use std::collections::HashSet;

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct YodaExpression {
    offended_nodes: HashSet<NodeId>,
}

impl Cop for YodaExpression {
    fn name(&self) -> &'static str {
        "Style/YodaExpression"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["*", "+", "&", "|", "^"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.offended_nodes.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method = node.method_name().unwrap_or_default();
        if !cx.cop_config_list("SupportedOperators").contains(&method) {
            return;
        }
        if !node.has_arguments() {
            return;
        }
        let (Some(lhs), Some(rhs)) = (node.receiver(), node.first_argument()) else {
            return;
        };
        if !yoda_expression_constant(lhs, rhs) || self.offended_ancestor(node) {
            return;
        }
        let message = format!("Non-literal operand (`{}`) should be first.", rhs.source());
        let source = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.swap(source, lhs.source_range(), rhs.source_range());
        });
        self.offended_nodes.insert(node.id());
    }
}

impl YodaExpression {
    fn offended_ancestor(&self, node: Node<'_>) -> bool {
        node.ancestors()
            .filter(|a| a.is_send())
            .any(|a| self.offended_nodes.contains(&a.id()))
    }
}

fn yoda_expression_constant(lhs: Node<'_>, rhs: Node<'_>) -> bool {
    constant_portion(lhs) && !constant_portion(rhs)
}

fn constant_portion(node: Node<'_>) -> bool {
    node.is_numeric() || node.is(NodeType::Const)
}
