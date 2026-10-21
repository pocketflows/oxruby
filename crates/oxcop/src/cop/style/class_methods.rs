//! Port of `lib/rubocop/cop/style/class_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct ClassMethods;

impl Cop for ClassMethods {
    fn name(&self) -> &'static str {
        "Style/ClassMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body() else {
            return;
        };
        let name = node.identifier().unwrap();
        if body.is(NodeType::Defs) {
            check_defs(cx, name, body);
        } else if body.is(NodeType::Begin) {
            for def_node in body.child_nodes().filter(|c| c.is(NodeType::Defs)) {
                check_defs(cx, name, def_node);
            }
        }
    }
}

fn check_defs(cx: &mut Investigation<'_, '_>, name: Node<'_>, node: Node<'_>) {
    let Some(receiver) = node.receiver() else {
        return;
    };
    if !name.same_as(receiver) {
        return;
    }
    let message = format!(
        "Use `self.{method}` instead of `{class}.{method}`.",
        method = node.method_name().unwrap_or(""),
        class = name.source()
    );
    let range = receiver.source_range();
    cx.add_offense_with_correction(receiver.loc().name.unwrap(), &message, |corrector| {
        corrector.replace(range, "self");
    });
}
