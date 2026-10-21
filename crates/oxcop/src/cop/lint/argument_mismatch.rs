//! Port of `lib/rubocop/cop/lint/argument_mismatch.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::indexed_method_arity::{
    Shape, arity_satisfied, expected_range, indexed_member_shape, positional_argument_count,
};
use crate::cop::mixin::project_index_help::{
    fully_resolved_index_ancestry, indexed_singleton_member, resolve_constant_in_index,
};
use crate::cop::{Cop, Investigation};
use crate::project_index::{Decl, ProjectIndex};

#[derive(Default)]
pub struct ArgumentMismatch;

impl Cop for ArgumentMismatch {
    fn name(&self) -> &'static str {
        "Lint/ArgumentMismatch"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = cx.project_index() else {
            return;
        };
        if !checkable_call(node) {
            return;
        }
        let Some((min, max, has_keywords)) = resolved_signature_shape(&index, node) else {
            return;
        };
        let Some(given) = positional_argument_count(node, has_keywords) else {
            return;
        };
        if arity_satisfied(given, min, max) {
            return;
        }
        let Some(selector) = node.loc().selector else {
            return;
        };
        cx.add_offense(selector, &message(node, given, min, max));
    }
}

/// `checkable_call?` (the `project_index` check is the caller's).
fn checkable_call(node: Node<'_>) -> bool {
    node.receiver().is_some_and(|r| r.is(NodeType::Const)) && !node.is_method("new")
}

fn message(node: Node<'_>, given: usize, min: usize, max: Option<usize>) -> String {
    format!(
        "Wrong number of arguments to `{}` (given {given}, expected {}).",
        node.method_name().unwrap_or_default(),
        expected_range(min, max)
    )
}

/// `resolved_signature_shape`: the shape of the called singleton method, or
/// `None` when the receiver or its ancestry is not resolved.
fn resolved_signature_shape(index: &ProjectIndex, node: Node<'_>) -> Option<Shape> {
    let declaration = resolve_constant_in_index(index, node.receiver()?)?;
    if !declaration.is_namespace() || !fully_resolved_index_ancestry(index, declaration, false) {
        return None;
    }
    singleton_signature_shape(index, declaration, node.method_name()?)
}

/// `singleton_signature_shape`: a call with an explicit receiver cannot
/// reach a private method, so its signature says nothing about the call.
fn singleton_signature_shape(
    index: &ProjectIndex,
    declaration: Decl,
    method_name: &str,
) -> Option<Shape> {
    let member = indexed_singleton_member(index, declaration, &format!("{method_name}()"))?;
    if !index.is_public(member) {
        return None;
    }
    indexed_member_shape(index, member)
}
