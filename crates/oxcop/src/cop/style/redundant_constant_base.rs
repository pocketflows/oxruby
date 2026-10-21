//! Port of `lib/rubocop/cop/style/redundant_constant_base.rb`.

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
        if lint_constant_resolution_cop_enabled(cx) || !(bad(node) || provably_unshadowed(cx, node)) {
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

/// `provably_unshadowed?`: with the project index, `::` inside a namespace
/// is also redundant when the first constant segment resolves to the same
/// declaration with and without the base.
fn provably_unshadowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(index) = cx.project_index() else {
        return false;
    };
    let Some(segment) = first_constant_segment(node) else {
        return false;
    };
    let mut nesting: Vec<String> = module_nesting_ancestors_of(node)
        .into_iter()
        .map(|ancestor| {
            ancestor
                .identifier()
                .and_then(|identifier| identifier.const_name())
                .unwrap_or_default()
        })
        .collect();
    nesting.reverse();
    !nesting.is_empty() && same_resolution(&index, segment, &nesting)
}

fn same_resolution(index: &crate::project_index::ProjectIndex, segment: &str, nesting: &[String]) -> bool {
    let relative = index.resolve_constant(segment, nesting);
    let absolute = index.resolve_constant(segment, &[]);
    match (relative, absolute) {
        (Some(relative), Some(absolute)) => index.name(relative) == index.name(absolute),
        _ => false,
    }
}

fn first_constant_segment(node: Node<'_>) -> Option<&str> {
    node.parent()
        .filter(|parent| parent.is(NodeType::Const))?
        .short_name()
}

fn bad(node: Node<'_>) -> bool {
    module_nesting_ancestors_of(node).is_empty()
}

fn module_nesting_ancestors_of(node: Node<'_>) -> Vec<Node<'_>> {
    node.ancestors()
        .filter(|ancestor| {
            matches!(ancestor.ty(), NodeType::Class | NodeType::Module)
                && !(ancestor.is(NodeType::Class) && used_in_super_class_part(node, *ancestor))
        })
        .collect()
}

fn used_in_super_class_part(node: Node<'_>, class_node: Node<'_>) -> bool {
    class_node
        .parent_class()
        .is_some_and(|p| p.descendants().any(|d| d == node))
}
