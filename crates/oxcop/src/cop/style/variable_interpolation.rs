//! Port of `lib/rubocop/cop/style/variable_interpolation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::interpolation::NODE_TYPES;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct VariableInterpolation;

impl Cop for VariableInterpolation {
    fn name(&self) -> &'static str {
        "Style/VariableInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    /// `on_node_with_interpolations(node)`.
    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for var_node in node
            .child_nodes()
            .filter(|n| n.is_variable() || n.is_reference())
        {
            let message = format!(
                "Replace interpolated variable `{0}` with expression `#{{{0}}}`.",
                var_node.source()
            );
            cx.add_offense_with_correction(var_node.source_range(), &message, |corrector| {
                corrector.replace(var_node.source_range(), format!("{{{}}}", var_node.source()));
            });
        }
    }
}
