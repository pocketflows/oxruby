//! Port of `lib/rubocop/cop/naming/predicate_prefix.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::project_index_help::{
    indexed_singleton_of, inherited_index_member, resolve_constant_in_index,
};
use crate::cop::{Cop, Investigation};

static SORBET_RETURN_TYPE: NodePattern =
    NodePattern::new("(block (send nil? :sig) args (send _ :returns $_type))");

#[derive(Default)]
pub struct PredicatePrefix;

impl Cop for PredicatePrefix {
    fn name(&self) -> &'static str {
        "Naming/PredicatePrefix"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_send() {
            self.on_send(cx, node);
            return;
        }
        let method_name = node.method_name().unwrap_or("");
        let offending: Vec<&str> = predicate_prefixes(cx)
            .into_iter()
            .filter(|prefix| {
                !(allowed_method_name(cx, method_name, prefix)
                    || (cx.cop_config_bool("UseSorbetSigs", false)
                        && !sorbet_sig(node, "T::Boolean")))
            })
            .collect();
        if offending.is_empty() || overrides_inherited_method(cx, node) {
            return;
        }
        for prefix in offending {
            let message = message(method_name, &expected_name(cx, method_name, prefix));
            cx.add_offense(node.loc().name.unwrap(), &message);
        }
    }
}

impl PredicatePrefix {
    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // dynamic_method_define: (send nil? #method_definition_macro? (sym $_) ...)
        if node.receiver().is_some() {
            return;
        }
        let Some(macro_name) = node.method_name() else {
            return;
        };
        if !cx
            .cop_config_list("MethodDefinitionMacros")
            .contains(&macro_name)
        {
            return;
        }
        let Some(first) = node.first_argument().filter(|a| a.is(NodeType::Sym)) else {
            return;
        };
        let method_name = first.sym_value().unwrap_or("");
        for prefix in predicate_prefixes(cx) {
            if allowed_method_name(cx, method_name, prefix) {
                continue;
            }
            let message = message(method_name, &expected_name(cx, method_name, prefix));
            cx.add_offense(first.source_range(), &message);
        }
    }
}

fn predicate_prefixes<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    cx.cop_config_list("NamePrefix")
}

fn sorbet_sig(node: Node<'_>, return_type: &str) -> bool {
    node.left_sibling()
        .and_then(|s| SORBET_RETURN_TYPE.match_node(s))
        .and_then(|c| c[0].node())
        .is_some_and(|t| t.source() == return_type)
}

fn allowed_method_name(cx: &Investigation<'_, '_>, method_name: &str, prefix: &str) -> bool {
    let matches_prefix = method_name.starts_with(prefix)
        && method_name[prefix.len()..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_digit() && c != '\n');
    !matches_prefix
        || method_name == expected_name(cx, method_name, prefix)
        || method_name.ends_with('=')
        || allowed_method(cx, method_name)
}

fn expected_name(cx: &Investigation<'_, '_>, method_name: &str, prefix: &str) -> String {
    let mut new_name = if cx.cop_config_list("ForbiddenPrefixes").contains(&prefix) {
        method_name.replacen(prefix, "", 1)
    } else {
        method_name.to_owned()
    };
    if !method_name.ends_with('?') {
        new_name.push('?');
    }
    new_name
}

fn message(method_name: &str, new_name: &str) -> String {
    format!("Rename `{method_name}` to `{new_name}`.")
}

/// `overrides_inherited_method?`: when `AllCops/UseProjectIndex` is
/// enabled, methods that override a method defined by an ancestor elsewhere
/// in the project are not reported.
fn overrides_inherited_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(index) = cx.project_index() else {
        return false;
    };
    let Some(namespace_node) = node
        .ancestors()
        .find(|a| a.is(NodeType::Class) || a.is(NodeType::Module))
    else {
        return false;
    };
    let Some(declaration) = namespace_node
        .identifier()
        .and_then(|identifier| resolve_constant_in_index(&index, identifier))
        .filter(|declaration| declaration.is_namespace())
    else {
        return false;
    };
    let scope = if node.is(NodeType::Defs) {
        indexed_singleton_of(&index, declaration)
    } else {
        Some(declaration)
    };
    scope.is_some_and(|scope| {
        inherited_index_member(
            &index,
            scope,
            &format!("{}()", node.method_name().unwrap_or_default()),
        )
    })
}
