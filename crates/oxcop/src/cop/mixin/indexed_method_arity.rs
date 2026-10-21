//! Port of `lib/rubocop/cop/mixin/indexed_method_arity.rb`: compares a call
//! site's arguments against a method signature from the project index.
//!
//! A signature is reduced to a shape, `(min, max, has_keywords)` where `max`
//! is `None` for a rest parameter, and the number of positional arguments a
//! call passes is counted with Ruby's semantics for trailing keyword hashes:
//! keywords passed to a method with no keyword parameters collapse into a
//! single positional hash.

use crate::ast::{Node, NodeType};
use crate::project_index::{Decl, DefKind, ProjectIndex, Signature};

/// `[min, max, has_keywords]`.
pub type Shape = (usize, Option<usize>, bool);

/// `arity_satisfied?`.
pub fn arity_satisfied(given: usize, min: usize, max: Option<usize>) -> bool {
    given >= min && max.is_none_or(|max| given <= max)
}

/// `expected_range`.
pub fn expected_range(min: usize, max: Option<usize>) -> String {
    match max {
        None => format!("{min}+"),
        Some(max) if max == min => min.to_string(),
        Some(max) => format!("{min}..{max}"),
    }
}

/// `indexed_member_shape`: the agreed shape of an indexed member's method
/// definitions, or `None` when the member is not backed by plain method
/// definitions (attr, alias), has no signature, forwards arguments, or its
/// definitions disagree on arity.
pub fn indexed_member_shape(index: &ProjectIndex, member: Decl) -> Option<Shape> {
    let definitions: Vec<_> = index
        .definitions(member)
        .into_iter()
        .filter(|def| def.kind == DefKind::Method)
        .collect();
    if definitions.is_empty() {
        return None;
    }
    let signatures: Vec<Signature> = definitions
        .into_iter()
        .flat_map(|def| index.signatures(def))
        .collect();
    signature_shape(&signatures)
}

/// `signature_shape`.
pub fn signature_shape(signatures: &[Signature]) -> Option<Shape> {
    let mut shapes: Vec<Shape> = Vec::new();
    for shape in signatures.iter().filter_map(shape_for) {
        if !shapes.contains(&shape) {
            shapes.push(shape);
        }
    }
    (shapes.len() == 1).then(|| shapes[0])
}

/// `shape_for`.
pub fn shape_for(signature: &Signature) -> Option<Shape> {
    if signature.forward_parameter() {
        return None;
    }
    let min = signature.positional_parameters() + signature.post_parameters();
    Some((
        min,
        maximum_positional(signature, min),
        keyword_parameters(signature),
    ))
}

fn maximum_positional(signature: &Signature, min: usize) -> Option<usize> {
    if signature.rest_positional_parameter() {
        return None;
    }
    Some(min + signature.optional_positional_parameters())
}

fn keyword_parameters(signature: &Signature) -> bool {
    signature.keyword_parameters() > 0
        || signature.optional_keyword_parameters() > 0
        || signature.rest_keyword_parameter()
}

/// `positional_argument_count`: the number of positional arguments the
/// call passes, or `None` when it cannot be determined statically
/// (argument forwarding).
pub fn positional_argument_count(node: Node<'_>, callee_has_keywords: bool) -> Option<usize> {
    let mut arguments = node.arguments();
    if forwards_arguments(&arguments) {
        return None;
    }
    if arguments
        .last()
        .is_some_and(|arg| arg.is(NodeType::BlockPass))
    {
        arguments.pop();
    }
    let Some(keyword_hash) = arguments.last().copied().and_then(keyword_hash) else {
        return Some(arguments.len());
    };
    if double_splat(keyword_hash) {
        return None;
    }
    // A keyword-style hash passed to a method with no keyword parameters is
    // received as a single positional hash argument.
    Some(if callee_has_keywords {
        arguments.len() - 1
    } else {
        arguments.len()
    })
}

fn forwards_arguments(arguments: &[Node<'_>]) -> bool {
    arguments.iter().any(|argument| {
        matches!(
            argument.ty(),
            NodeType::Splat
                | NodeType::ForwardedArgs
                | NodeType::ForwardedRestarg
                | NodeType::ForwardedKwrestarg
        )
    })
}

fn keyword_hash(node: Node<'_>) -> Option<Node<'_>> {
    (node.is(NodeType::Hash) && !node.has_braces()).then_some(node)
}

fn double_splat(hash_node: Node<'_>) -> bool {
    hash_node
        .child_nodes()
        .any(|child| child.is(NodeType::Kwsplat))
}
