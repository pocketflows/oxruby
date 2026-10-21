//! Port of `lib/rubocop/cop/lint/constant_resolution.rb`.

use crate::ast::{MapKind, Node, NodeType};
use crate::cop::mixin::project_index_help::lexical_nesting_of;
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
        if let Some(index) = cx.project_index()
            && !ambiguous_resolution(&index, node)
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

/// `ambiguous_resolution?`: with the project index, only genuinely
/// ambiguous references are reported: the constant resolves to a different
/// declaration through the lexical nesting than it would fully qualified.
fn ambiguous_resolution(index: &crate::project_index::ProjectIndex, node: Node<'_>) -> bool {
    let name = node.short_name().unwrap_or_default();
    let relative = index.resolve_constant(name, &lexical_nesting_of(node));
    let absolute = index.resolve_constant(name, &[]);
    match (relative, absolute) {
        (Some(relative), Some(absolute)) => index.name(relative) != index.name(absolute),
        _ => false,
    }
}
