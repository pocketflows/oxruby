//! Port of `lib/rubocop/cop/style/static_class.rb`.
//!
//! `subclassed_in_project?` needs the `rubydex` project index, which isn't
//! supported; RuboCop behaves the same without one.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::visibility_help::node_visibility;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Prefer modules to classes with only class methods.";

#[derive(Default)]
pub struct StaticClass;

impl Cop for StaticClass {
    fn name(&self) -> &'static str {
        "Style/StaticClass"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, class_node: Node<'a>) {
        if class_node.parent_class().is_some() || !class_convertible_to_module(class_node) {
            return;
        }
        cx.add_offense_with_correction(class_node.source_range(), MSG, |corrector| {
            autocorrect(corrector, class_node);
        });
    }
}

fn autocorrect(corrector: &mut Corrector, class_node: Node<'_>) {
    corrector.replace(class_node.loc().keyword.unwrap(), "module");
    corrector.insert_after(class_node.loc().name.unwrap(), "\nmodule_function\n");
    for node in class_elements(class_node) {
        if node.is(NodeType::Defs) {
            corrector.remove(range_between(
                node.receiver().unwrap().source_range().begin_pos,
                node.loc().name.unwrap().begin_pos,
            ));
        } else if node.is(NodeType::Sclass) {
            corrector.remove(range_between(
                node.loc().keyword.unwrap().begin_pos,
                node.identifier().unwrap().source_range().end_pos,
            ));
            corrector.remove(node.loc().end.unwrap());
        }
    }
}

fn class_convertible_to_module(class_node: Node<'_>) -> bool {
    let nodes = class_elements(class_node);
    if nodes.is_empty() {
        return false;
    }
    nodes.iter().all(|node| {
        (node_visibility(*node) == "public" && node.is(NodeType::Defs))
            || sclass_convertible_to_module(*node)
            || node.is_equals_asgn()
            || (node.is_send() && node.is_method("extend"))
    })
}

fn sclass_convertible_to_module(node: Node<'_>) -> bool {
    if !node.is(NodeType::Sclass) {
        return false;
    }
    class_elements(node).iter().all(|child| {
        node_visibility(*child) == "public" && (child.is(NodeType::Def) || child.is_equals_asgn())
    })
}

fn class_elements(class_node: Node<'_>) -> Vec<Node<'_>> {
    match class_node.body() {
        None => Vec::new(),
        Some(body) if body.is(NodeType::Begin) => body.child_nodes().collect(),
        Some(body) => vec![body],
    }
}
