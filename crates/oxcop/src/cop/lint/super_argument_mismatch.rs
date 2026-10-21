//! Port of `lib/rubocop/cop/lint/super_argument_mismatch.rb`.

use std::path::Path;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::indexed_method_arity::{
    Shape, arity_satisfied, expected_range, indexed_member_shape, positional_argument_count,
};
use crate::cop::mixin::project_index_help::{
    FILE_URI_PREFIX, fully_resolved_index_ancestry, lexical_nesting_of, same_file,
};
use crate::cop::{Cop, Investigation};
use crate::project_index::{Decl, ProjectIndex};

/// Blocks that change what method (or scope) `super` refers to, or in
/// which `super` is not statically resolvable.
const CONTEXT_CHANGING_BLOCKS: &[&str] = &[
    "define_method",
    "define_singleton_method",
    "class_eval",
    "class_exec",
    "module_eval",
    "module_exec",
    "instance_eval",
    "instance_exec",
    "refine",
];

/// Methods the index attributes to the root namespaces are not trustworthy
/// super targets.
const BUILTIN_ROOTS: &[&str] = &["Object", "Kernel", "BasicObject"];

#[derive(Default)]
pub struct SuperArgumentMismatch;

impl Cop for SuperArgumentMismatch {
    fn name(&self) -> &'static str {
        "Lint/SuperArgumentMismatch"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Super]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = cx.project_index() else {
            return;
        };
        let Some(def_node) = enclosing_instance_method(node) else {
            return;
        };
        let Some(((min, max, has_keywords), parent)) = resolved_super_shape(cx, &index, def_node)
        else {
            return;
        };
        let Some(given) = positional_argument_count(node, has_keywords) else {
            return;
        };
        if arity_satisfied(given, min, max) {
            return;
        }
        let Some(keyword) = node.loc().keyword else {
            return;
        };
        let message = format!(
            "Wrong number of arguments to `super` (given {given}, expected {} for `{}#{}`).",
            expected_range(min, max),
            index.name(parent),
            def_node.method_name().unwrap_or_default()
        );
        cx.add_offense(keyword, &message);
    }
}

/// `enclosing_instance_method`: the `def` node whose method the `super`
/// call re-dispatches, or `None` when the context is not statically known.
fn enclosing_instance_method(node: Node<'_>) -> Option<Node<'_>> {
    for ancestor in node.ancestors() {
        match ancestor.ty() {
            NodeType::Def => return Some(ancestor),
            NodeType::Defs | NodeType::Sclass => return None,
            _ if ancestor.is_any_block() => {
                if ancestor
                    .method_name()
                    .is_some_and(|name| CONTEXT_CHANGING_BLOCKS.contains(&name))
                {
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

/// `resolved_super_shape`.
fn resolved_super_shape(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    def_node: Node<'_>,
) -> Option<(Shape, Decl)> {
    let declaration = enclosing_class_declaration(cx, index, def_node)?;
    if !fully_resolved_index_ancestry(index, declaration, true) {
        return None;
    }
    super_method_shape(index, declaration, def_node.method_name()?)
}

/// `enclosing_class_declaration`.
fn enclosing_class_declaration(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    def_node: Node<'_>,
) -> Option<Decl> {
    let class_node = def_node.ancestors().find(|a| {
        matches!(a.ty(), NodeType::Class | NodeType::Module | NodeType::Sclass) || a.is_any_block()
    })?;
    if !class_node.is(NodeType::Class) {
        return None;
    }
    let declaration = index.get(&lexical_nesting_of(def_node).join("::"))?;
    if !declaration.is_namespace() || !declared_at(cx, index, declaration, class_node) {
        return None;
    }
    Some(declaration)
}

/// `declared_at?`.
fn declared_at(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    declaration: Decl,
    class_node: Node<'_>,
) -> bool {
    let current = cx.processed_source.file_path();
    index.definitions(declaration).into_iter().any(|definition| {
        index.location(definition).is_some_and(|location| {
            location.uri.starts_with(FILE_URI_PREFIX)
                && location.start_line as usize + 1 == class_node.first_line()
                && location
                    .file_path
                    .is_some_and(|path| same_file(&path, Path::new(&current)))
        })
    })
}

/// `super_method_shape`: the shape of the first implementation of
/// `method_name` after the enclosing class in the method resolution order.
fn super_method_shape(
    index: &ProjectIndex,
    declaration: Decl,
    method_name: &str,
) -> Option<(Shape, Decl)> {
    let ancestors = index.ancestors(declaration);
    let name = index.name(declaration);
    let position = ancestors.iter().position(|a| index.name(*a) == name)?;
    let member_name = format!("{method_name}()");
    for ancestor in ancestors.into_iter().skip(position + 1) {
        if BUILTIN_ROOTS.contains(&index.name(ancestor).as_str()) {
            return None;
        }
        let Some(member) = index.member(ancestor, &member_name) else {
            continue;
        };
        return indexed_member_shape(index, member).map(|shape| (shape, ancestor));
    }
    None
}
