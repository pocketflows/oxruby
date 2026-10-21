//! Port of `lib/rubocop/cop/lint/constant_reassignment.rb`.

use std::collections::HashMap;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::project_index_help::prior_definition_in_other_file;
use crate::cop::{Cop, Investigation};

static REMOVE_CONSTANT: NodePattern =
    NodePattern::new("(send {nil? self} :remove_const ({sym str} $_))");

#[derive(Default)]
pub struct ConstantReassignment {
    /// `@constant_definitions` (kept for the cop instance's lifetime).
    constant_definitions: HashMap<String, &'static str>,
}

impl Cop for ConstantReassignment {
    fn name(&self) -> &'static str {
        "Lint/ConstantReassignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Class,
            NodeType::Module,
            NodeType::Casgn,
            NodeType::Send,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["remove_const"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Class | NodeType::Module => {
                if !unconditional_definition(node) {
                    return;
                }
                let kind = if node.is(NodeType::Class) {
                    "class"
                } else {
                    "module"
                };
                self.constant_definitions
                    .entry(definition_name(node))
                    .or_insert(kind);
            }
            NodeType::Casgn => self.on_casgn(cx, node),
            _ => self.on_send(node),
        }
    }
}

impl ConstantReassignment {
    fn on_casgn<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !fixed_constant_path(node) {
            return;
        }
        if !simple_assignment(node) {
            return;
        }
        let name = fully_qualified_constant_name(node);
        if self.constant_definitions.contains_key(&name) {
            let message = format!(
                "Constant `{}` is already assigned in this namespace.",
                constant_display_name(node)
            );
            cx.add_offense(node.source_range(), &message);
            return;
        }
        self.constant_definitions.insert(name.clone(), "casgn");
        report_cross_file_collision(cx, node, &name, &constant_display_name(node));
    }

    fn on_send(&mut self, node: Node<'_>) {
        let Some(captures) = REMOVE_CONSTANT.match_node(node) else {
            return;
        };
        let Some(constant) = captures[0].sym().or_else(|| captures[0].str()) else {
            return;
        };
        let namespaces = ancestor_namespaces(node);
        if namespaces.is_empty() {
            return;
        }
        self.constant_definitions
            .remove(&fully_qualified_name_for(&namespaces, constant));
    }
}

/// `ConstNode#each_path`: the namespace nodes, outermost first.
fn each_path(node: Node<'_>) -> Vec<Node<'_>> {
    let mut descendants = Vec::new();
    let mut last_node = node;
    while let Some(child) = last_node.child(0) {
        descendants.push(child);
        if !child.is(NodeType::Const) {
            break;
        }
        last_node = child;
    }
    descendants.reverse();
    descendants
}

fn fixed_constant_path(node: Node<'_>) -> bool {
    each_path(node)
        .iter()
        .all(|path| path.is_any(&[NodeType::Cbase, NodeType::Const, NodeType::SelfNode]))
}

fn simple_assignment(node: Node<'_>) -> bool {
    for ancestor in node.ancestors() {
        if ancestor.is_any(&[NodeType::Module, NodeType::Class]) {
            return true;
        }
        let ok = ancestor.is(NodeType::Begin)
            || ancestor.is_literal()
            || ancestor.is(NodeType::Casgn)
            || ancestor.is_any(&[NodeType::Masgn, NodeType::Mlhs])
            || freeze_method(ancestor);
        if !ok {
            return false;
        }
    }
    true
}

fn freeze_method(node: Node<'_>) -> bool {
    node.is_send() && node.is_method("freeze")
}

fn short_name(node: Node<'_>) -> String {
    node.short_name().unwrap_or_default().to_owned()
}

fn fully_qualified_constant_name(node: Node<'_>) -> String {
    if node.is_absolute() {
        let namespace = node
            .namespace()
            .filter(|ns| ns.is(NodeType::Const))
            .map(|ns| ns.source())
            .unwrap_or_default();
        format!("{namespace}::{}", short_name(node))
    } else {
        let mut namespaces = ancestor_namespaces(node);
        namespaces.extend(constant_namespaces(node));
        fully_qualified_name_for(&namespaces, &short_name(node))
    }
}

fn fully_qualified_name_for(namespaces: &[String], constant: &str) -> String {
    let mut parts = vec![String::new()];
    parts.extend(namespaces.iter().cloned());
    parts.push(constant.to_owned());
    parts.join("::")
}

fn constant_display_name(node: Node<'_>) -> String {
    let mut parts = constant_namespaces(node);
    parts.push(short_name(node));
    parts.join("::")
}

fn constant_namespaces(node: Node<'_>) -> Vec<String> {
    each_path(node)
        .into_iter()
        .filter(|p| p.is(NodeType::Const))
        .map(short_name)
        .collect()
}

fn ancestor_namespaces(node: Node<'_>) -> Vec<String> {
    let mut ancestors: Vec<Node<'_>> = node
        .ancestors()
        .filter(|a| a.is_any(&[NodeType::Class, NodeType::Module]))
        .collect();
    ancestors.reverse();
    let mut namespaces = Vec::new();
    for ancestor in ancestors {
        if let Some(identifier) = ancestor.identifier() {
            append_namespaces(&mut namespaces, identifier);
        }
    }
    namespaces
}

fn append_namespaces(namespaces: &mut Vec<String>, identifier: Node<'_>) {
    if identifier.is_absolute() {
        namespaces.clear();
    }
    namespaces.extend(identifier_namespaces(identifier));
    namespaces.push(short_name(identifier));
}

fn unconditional_definition(node: Node<'_>) -> bool {
    node.ancestors()
        .all(|a| a.is_any(&[NodeType::Begin, NodeType::Module, NodeType::Class]))
}

fn definition_name(node: Node<'_>) -> String {
    let mut namespaces = ancestor_namespaces(node);
    if let Some(identifier) = node.identifier() {
        append_namespaces(&mut namespaces, identifier);
    }
    let constant = namespaces.pop().unwrap_or_default();
    fully_qualified_name_for(&namespaces, &constant)
}

fn identifier_namespaces(identifier: Node<'_>) -> Vec<String> {
    each_path(identifier)
        .into_iter()
        .filter(|p| p.is(NodeType::Const))
        .map(short_name)
        .collect()
}


/// `report_cross_file_collision`.
fn report_cross_file_collision(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    fully_qualified_name: &str,
    display_name: &str,
) {
    let Some(index) = cx.project_index() else {
        return;
    };
    let Some(declaration) =
        index.get(fully_qualified_name.strip_prefix("::").unwrap_or(fully_qualified_name))
    else {
        return;
    };
    let definitions = index.definitions(declaration);
    let Some(prior) = prior_definition_in_other_file(cx, &index, &definitions) else {
        return;
    };
    let Some(path) = index.location(prior).and_then(|location| location.file_path) else {
        return;
    };
    let message = format!(
        "Constant `{display_name}` is already assigned in `{}`.",
        path.display()
    );
    cx.add_offense(node.source_range(), &message);
}
