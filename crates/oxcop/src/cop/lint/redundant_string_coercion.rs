//! Port of `lib/rubocop/cop/lint/redundant_string_coercion.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::interpolation;
use crate::cop::{Cop, Investigation};

static TO_S_WITHOUT_ARGS: NodePattern = NodePattern::new("(call _ :to_s)");

#[derive(Default)]
pub struct RedundantStringCoercion;

impl Cop for RedundantStringCoercion {
    fn name(&self) -> &'static str {
        "Lint/RedundantStringCoercion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Dstr,
            NodeType::Xstr,
            NodeType::Dsym,
            NodeType::Regexp,
            NodeType::Send,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["print", "puts", "warn"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Send) {
            on_send(cx, node);
            return;
        }
        for begin_node in interpolation::interpolations(node) {
            on_interpolation(cx, begin_node);
        }
    }
}

fn on_interpolation(cx: &mut Investigation<'_, '_>, begin_node: Node<'_>) {
    let Some(final_node) = begin_node.child_nodes().next_back() else {
        return;
    };
    if !TO_S_WITHOUT_ARGS.is_match(final_node) {
        return;
    }

    register_offense(cx, final_node, "interpolation");
}

fn on_send(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if node.receiver().is_some() {
        return;
    }

    for child in node.child_nodes().filter(|c| c.is_call()) {
        if !child.is_method("to_s") || child.has_arguments() {
            continue;
        }

        register_offense(
            cx,
            child,
            &format!("`{}`", node.method_name().unwrap_or_default()),
        );
    }
}

fn register_offense(cx: &mut Investigation<'_, '_>, node: Node<'_>, context: &str) {
    let receiver = node.receiver();
    let message = if receiver.is_some() {
        format!("Redundant use of `Object#to_s` in {context}.")
    } else {
        format!("Use `self` instead of `Object#to_s` in {context}.")
    };
    let Some(selector) = node.loc().selector else {
        return;
    };
    cx.add_offense_with_correction(selector, &message, |corrector| {
        let replacement = receiver.map_or("self", |r| r.source());
        corrector.replace(node.source_range(), replacement);
    });
}
