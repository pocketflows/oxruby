//! Port of `lib/rubocop/cop/lint/missing_super.rb`.
//!
//! Without a project index (`rubydex` isn't supported),
//! `index_verified_stateless_ancestry?` is always false, as in RuboCop
//! without one.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const CONSTRUCTOR_MSG: &str = "Call `super` to initialize state of the parent class.";
const CALLBACK_MSG: &str = "Call `super` to invoke callback defined in the parent class.";

const STATELESS_CLASSES: &[&str] = &["BasicObject", "Object"];

const CALLBACKS: &[&str] = &[
    "inherited",
    "method_added",
    "method_removed",
    "method_undefined",
    "singleton_method_added",
    "singleton_method_removed",
    "singleton_method_undefined",
];

static CLASS_NEW_BLOCK: NodePattern =
    NodePattern::new("(any_block (send (const {nil? cbase} :Class) :new $_) ...)");

#[derive(Default)]
pub struct MissingSuper;

impl Cop for MissingSuper {
    fn name(&self) -> &'static str {
        "Lint/MissingSuper"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Defs) {
            if !callback_method_def(node) || contains_super(node) {
                return;
            }
            cx.add_offense(node.source_range(), CALLBACK_MSG);
            return;
        }
        if !offender(node) {
            return;
        }
        if node.is_method("initialize") && inside_class_with_stateful_parent(cx, node) {
            cx.add_offense(node.source_range(), CONSTRUCTOR_MSG);
        } else if callback_method_def(node) {
            cx.add_offense(node.source_range(), CALLBACK_MSG);
        }
    }
}

fn offender(node: Node<'_>) -> bool {
    (node.is_method("initialize") || callback_method_def(node)) && !contains_super(node)
}

fn callback_method_def(node: Node<'_>) -> bool {
    if !node.method_name().is_some_and(|m| CALLBACKS.contains(&m)) {
        return false;
    }
    node.ancestors().any(|a| {
        a.is_any(&[NodeType::Class, NodeType::Sclass, NodeType::Module])
    })
}

fn contains_super(node: Node<'_>) -> bool {
    node.descendants()
        .any(|d| d.is_any(&[NodeType::Super, NodeType::Zsuper]))
}

fn inside_class_with_stateful_parent(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if let Some(block_node) = node.ancestors().find(|a| a.is_any_block()) {
        let Some(super_class) = CLASS_NEW_BLOCK
            .match_node(block_node)
            .and_then(|c| c[0].node())
        else {
            return false;
        };
        !allowed_class(cx, super_class)
    } else if let Some(class_node) = node.ancestors().find(|a| a.is(NodeType::Class)) {
        class_node
            .parent_class()
            .is_some_and(|parent_class| !allowed_class(cx, parent_class))
    } else {
        false
    }
}

fn allowed_class(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(name) = node.const_name() else {
        return false;
    };
    STATELESS_CLASSES.contains(&name.as_str())
        || cx.cop_config_list("AllowedParentClasses").contains(&name.as_str())
}
