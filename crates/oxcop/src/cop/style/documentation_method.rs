//! Port of `lib/rubocop/cop/style/documentation_method.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::def_node::non_public;
use crate::cop::mixin::documentation_comment::documentation_comment;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Missing method documentation comment.";

static MODIFIER_NODE: NodePattern =
    NodePattern::new("(send nil? {:module_function :ruby2_keywords} ...)");

#[derive(Default)]
pub struct DocumentationMethod;

impl Cop for DocumentationMethod {
    fn name(&self) -> &'static str {
        "Style/DocumentationMethod"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_method("initialize") {
            return;
        }
        let method_name = node.method_name().unwrap_or("");
        if allowed_methods(cx).contains(&method_name) {
            return;
        }
        match node.parent() {
            Some(parent) if MODIFIER_NODE.is_match(parent) => check(cx, parent),
            _ => check(cx, node),
        }
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if non_public(node) && !require_for_non_public_methods(cx) {
        return;
    }
    if documentation_comment(cx, node) {
        return;
    }
    cx.add_offense(node.source_range(), MSG);
}

fn require_for_non_public_methods(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("RequireForNonPublicMethods", false)
}

/// `cop_config.fetch('AllowedMethods', []).map(&:to_sym)`.
fn allowed_methods<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    cx.cop_config_list("AllowedMethods")
        .into_iter()
        .map(|name| name.strip_prefix(':').unwrap_or(name))
        .collect()
}
