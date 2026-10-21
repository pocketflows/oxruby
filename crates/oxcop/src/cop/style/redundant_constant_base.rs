//! Port of `lib/rubocop/cop/style/redundant_constant_base.rb`. The project
//! index (`rubydex`) isn't supported, so `provably_unshadowed?` is always
//! false, as in RuboCop without one.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove redundant `::`.";

#[derive(Default)]
pub struct RedundantConstantBase;

impl Cop for RedundantConstantBase {
    fn name(&self) -> &'static str {
        "Style/RedundantConstantBase"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Cbase]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if lint_constant_resolution_cop_enabled(cx) || !bad(node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.remove(node.source_range());
        });
    }
}

/// `config.for_cop('Lint/ConstantResolution').fetch('Enabled', false)`.
fn lint_constant_resolution_cop_enabled(cx: &Investigation<'_, '_>) -> bool {
    match cx.config.for_cop("Lint/ConstantResolution").get("Enabled") {
        None | Some(Value::Bool(false)) | Some(Value::Null) => false,
        Some(_) => true,
    }
}

fn bad(node: Node<'_>) -> bool {
    !node.ancestors().any(|ancestor| {
        matches!(ancestor.ty(), NodeType::Class | NodeType::Module)
            && !(ancestor.is(NodeType::Class) && used_in_super_class_part(node, ancestor))
    })
}

fn used_in_super_class_part(node: Node<'_>, class_node: Node<'_>) -> bool {
    class_node
        .parent_class()
        .is_some_and(|p| p.descendants().any(|d| d == node))
}
