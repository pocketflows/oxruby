//! Port of `lib/rubocop/cop/lint/deprecated_reference.rb`.

use std::collections::HashMap;

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::project_index_help::{indexed_singleton_member, resolve_constant_in_index};
use crate::cop::{Cop, Investigation};
use crate::project_index::{Decl, ProjectIndex};

const DEPRECATED_TAG: &str = "@deprecated";

#[derive(Default)]
pub struct DeprecatedReference {
    namespace_cache: HashMap<NodeId, Option<Decl>>,
}

impl Cop for DeprecatedReference {
    fn name(&self) -> &'static str {
        "Lint/DeprecatedReference"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Const]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.namespace_cache.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = cx.project_index() else {
            return;
        };
        if node.is(NodeType::Const) {
            self.on_const(cx, &index, node);
        } else {
            self.on_send(cx, &index, node);
        }
    }
}

impl DeprecatedReference {
    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, index: &ProjectIndex, node: Node<'a>) {
        let Some(declaration) = self.method_declaration_for(index, node) else {
            return;
        };
        if !deprecated(index, declaration) || within_deprecated_definition(cx, node) {
            return;
        }
        let Some(selector) = node.loc().selector else {
            return;
        };
        let message = format!(
            "Method `{}` is deprecated{}",
            node.method_name().unwrap_or_default(),
            detail_for(index, declaration)
        );
        cx.add_offense(selector, &message);
    }

    fn on_const<'a>(&mut self, cx: &mut Investigation<'a, '_>, index: &ProjectIndex, node: Node<'a>) {
        if node
            .parent()
            .is_some_and(|parent| parent.defined_module_name().is_some())
        {
            return;
        }
        let Some(declaration) = resolve_constant_in_index(index, node) else {
            return;
        };
        if !deprecated(index, declaration) || within_deprecated_definition(cx, node) {
            return;
        }
        let message = format!(
            "Constant `{}` is deprecated{}",
            node.const_name().unwrap_or_default(),
            detail_for(index, declaration)
        );
        cx.add_offense(node.source_range(), &message);
    }

    fn method_declaration_for(&mut self, index: &ProjectIndex, node: Node<'_>) -> Option<Decl> {
        match node.receiver() {
            None => self.implicit_receiver_declaration(index, node),
            Some(receiver) if receiver.is(NodeType::SelfNode) => {
                self.implicit_receiver_declaration(index, node)
            }
            Some(receiver) if receiver.is(NodeType::Const) => {
                const_receiver_declaration(index, node, receiver)
            }
            Some(_) => None,
        }
    }

    fn implicit_receiver_declaration(&mut self, index: &ProjectIndex, node: Node<'_>) -> Option<Decl> {
        let namespace = self.enclosing_namespace(index, node)?;
        if !namespace.is_namespace() {
            return None;
        }
        let member = format!("{}()", node.method_name()?);
        if singleton_context(node) {
            indexed_singleton_member(index, namespace, &member)
        } else {
            index.find_member(namespace, &member)
        }
    }

    fn enclosing_namespace(&mut self, index: &ProjectIndex, node: Node<'_>) -> Option<Decl> {
        let Some(namespace_node) = node
            .ancestors()
            .find(|a| a.is(NodeType::Class) || a.is(NodeType::Module))
        else {
            return index.get("Object");
        };
        *self
            .namespace_cache
            .entry(namespace_node.id())
            .or_insert_with(|| {
                namespace_node
                    .identifier()
                    .and_then(|identifier| resolve_constant_in_index(index, identifier))
            })
    }
}

fn const_receiver_declaration(index: &ProjectIndex, node: Node<'_>, receiver: Node<'_>) -> Option<Decl> {
    let namespace = resolve_constant_in_index(index, receiver)?;
    if !namespace.is_namespace() {
        return None;
    }
    indexed_singleton_member(index, namespace, &format!("{}()", node.method_name()?))
}

/// `singleton_context?`: whether an implicit-receiver call runs with the
/// class or module itself as `self` (directly in a namespace body, in a
/// singleton method, or inside `class << self`) rather than inside an
/// instance method.
fn singleton_context(node: Node<'_>) -> bool {
    let mut instance_def_seen = false;
    for ancestor in node.ancestors() {
        match ancestor.ty() {
            NodeType::Defs | NodeType::Sclass => return true,
            NodeType::Def => instance_def_seen = true,
            NodeType::Class | NodeType::Module => return !instance_def_seen,
            _ => {}
        }
    }
    false
}

/// `deprecated?`: every definition is deprecated (and there is one).
fn deprecated(index: &ProjectIndex, declaration: Decl) -> bool {
    let mut any = false;
    for definition in index.definitions(declaration) {
        if !index.is_deprecated(definition) {
            return false;
        }
        any = true;
    }
    any
}

/// `within_deprecated_definition?`: references from a definition that is
/// itself documented as deprecated are allowed.
fn within_deprecated_definition(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let processed_source = cx.processed_source;
    let comments = processed_source.ast_with_comments();
    node.ancestors()
        .filter(|a| {
            a.is_any_def() || a.is(NodeType::Class) || a.is(NodeType::Module)
        })
        .any(|ancestor| {
            comments.get(&ancestor.id()).is_some_and(|indices| {
                indices.iter().any(|&i| {
                    processed_source.comments()[i].text.contains(DEPRECATED_TAG)
                })
            })
        })
}

fn detail_for(index: &ProjectIndex, declaration: Decl) -> String {
    match deprecation_text(index, declaration) {
        Some(text) if !text.is_empty() => {
            if text.ends_with('.') {
                format!(": {text}")
            } else {
                format!(": {text}.")
            }
        }
        _ => ".".to_owned(),
    }
}

/// `deprecation_text`: `tag_comment[DEPRECATION_DETAIL, 1]&.strip` of the
/// first definition comment carrying the tag.
fn deprecation_text(index: &ProjectIndex, declaration: Decl) -> Option<String> {
    let tag_comment = index
        .definitions(declaration)
        .into_iter()
        .find_map(|definition| {
            index
                .comments(definition)
                .into_iter()
                .find(|string| string.contains(DEPRECATED_TAG))
        })?;
    let detail = crate::regex!(r"@deprecated\s+(.+)");
    let captures = detail.captures(&tag_comment)?;
    Some(ruby_strip(captures.at(1)?).to_owned())
}

/// `String#strip`: whitespace and NUL.
fn ruby_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '\x0b')
}
