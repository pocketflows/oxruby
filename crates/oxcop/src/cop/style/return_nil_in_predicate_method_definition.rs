//! Port of `lib/rubocop/cop/style/return_nil_in_predicate_method_definition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Return `false` instead of `nil` in predicate methods.";

#[derive(Default)]
pub struct ReturnNilInPredicateMethodDefinition;

impl Cop for ReturnNilInPredicateMethodDefinition {
    fn name(&self) -> &'static str {
        "Style/ReturnNilInPredicateMethodDefinition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_predicate_method() {
            return;
        }
        let name = node.method_name().unwrap_or("");
        if allowed_method(cx, name) || matches_allowed_pattern(cx, name) {
            return;
        }
        let Some(body) = node.body() else {
            return;
        };
        let returns: Vec<Node<'a>> = body
            .descendants()
            .filter(|d| d.is(NodeType::Return))
            .collect();
        for return_node in returns {
            // {(return) (return (nil))}
            let is_return_nil = match return_node.children() {
                [] => true,
                [_] => return_node.child(0).is_some_and(|c| c.is(NodeType::Nil)),
                _ => false,
            };
            if is_return_nil {
                register_offense(cx, return_node, "return false");
            }
        }
        handle_implicit_return_values(cx, Some(body));
    }
}

fn last_node_of_type(node: Option<Node<'_>>, ty: NodeType) -> Option<Node<'_>> {
    let node = node?;
    if node.is(ty) {
        return Some(node);
    }
    if !node.is(NodeType::Begin) {
        return None;
    }
    let last_child = node.child_from_end(1)?;
    last_child.is(ty).then_some(last_child)
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, offense_node: Node<'a>, replacement: &str) {
    cx.add_offense_with_correction(offense_node.source_range(), MSG, |corrector| {
        corrector.replace(offense_node.source_range(), replacement);
    });
}

fn handle_implicit_return_values<'a>(cx: &mut Investigation<'a, '_>, node: Option<Node<'a>>) {
    if let Some(if_node) = last_node_of_type(node, NodeType::If) {
        handle_implicit_return_values(cx, if_node.if_branch());
        handle_implicit_return_values(cx, if_node.else_branch());
    }
    if let Some(nil_node) = last_node_of_type(node, NodeType::Nil) {
        register_offense(cx, nil_node, "false");
    }
}
