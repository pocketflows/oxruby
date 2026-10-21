//! Port of `lib/rubocop/cop/lint/inherit_exception.rb`.
//!
//! Without a project index (`rubydex` isn't supported), `inherits_exception_via`
//! is always `nil`, as in RuboCop without one.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

static CLASS_NEW_CALL: NodePattern =
    NodePattern::new("(send (const {cbase nil?} :Class) :new $(const {cbase nil?} _))");

#[derive(Default)]
pub struct InheritException;

impl Cop for InheritException {
    fn name(&self) -> &'static str {
        "Lint/InheritException"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Class) {
            on_class(cx, node);
        } else {
            on_send(cx, node);
        }
    }
}

fn on_class<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(parent_class) = node.parent_class() else {
        return;
    };
    if exception_class(parent_class) {
        if inherit_exception_class_with_omitted_namespace(node) {
            return;
        }
        let preferred = preferred_base_class(cx);
        cx.add_offense_with_correction(parent_class.source_range(), &message(preferred), |corrector| {
            corrector.replace(parent_class.source_range(), preferred);
        });
    }
}

fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(constant) = CLASS_NEW_CALL.match_node(node).and_then(|c| c[0].node()) else {
        return;
    };
    if !exception_class(constant) {
        return;
    }
    let preferred = preferred_base_class(cx);
    cx.add_offense_with_correction(constant.source_range(), &message(preferred), |corrector| {
        corrector.replace(constant.source_range(), preferred);
    });
}

fn message(prefer: &str) -> String {
    format!("Inherit from `{prefer}` instead of `Exception`.")
}

fn exception_class(class_node: Node<'_>) -> bool {
    class_node.const_name().as_deref() == Some("Exception")
}

fn inherit_exception_class_with_omitted_namespace(class_node: Node<'_>) -> bool {
    let parent_class = class_node.parent_class().unwrap();
    if parent_class.namespace().is_some_and(|ns| ns.is(NodeType::Cbase)) {
        return false;
    }
    class_node
        .left_siblings()
        .into_iter()
        .any(|sibling| sibling.identifier().is_some_and(exception_class))
}

fn preferred_base_class(cx: &Investigation<'_, '_>) -> &'static str {
    match style(cx) {
        "runtime_error" => "RuntimeError",
        _ => "StandardError",
    }
}
