//! Port of `lib/rubocop/cop/lint/constant_resolution.rb`.
//!
//! Without a project index (`rubydex` isn't supported) every unqualified
//! constant is reported, as in RuboCop without one.

use crate::ast::{MapKind, Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Fully qualify this constant to avoid possibly ambiguous resolution.";

#[derive(Default)]
pub struct ConstantResolution;

impl Cop for ConstantResolution {
    fn name(&self) -> &'static str {
        "Lint/ConstantResolution"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !unqualified_const(cx, node)
            || node
                .parent()
                .is_some_and(|p| p.defined_module_name().is_some())
            // `node.loc.nil?`: `__ENCODING__`'s `Encoding` has no source map.
            || node.loc().kind == MapKind::None
        {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

/// `(const nil? #const_name?)`
fn unqualified_const(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.child_value(0).is_some_and(|c| c.is_nil())
        && node.child_sym(1).is_some_and(|name| const_name(cx, name))
}

fn const_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    let allowed_names = cx.cop_config_list("Only");
    (allowed_names.is_empty() || allowed_names.contains(&name))
        && !cx.cop_config_list("Ignore").contains(&name)
}
