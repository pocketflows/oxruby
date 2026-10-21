//! Port of `lib/rubocop/cop/style/class_vars.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

fn message(class_var: &str) -> String {
    format!("Replace class var {class_var} with a class instance var.")
}

#[derive(Default)]
pub struct ClassVars;

impl Cop for ClassVars {
    fn name(&self) -> &'static str {
        "Style/ClassVars"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Cvasgn, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["class_variable_set"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Cvasgn) {
            let name = node.child_sym(0).unwrap_or("");
            cx.add_offense(node.loc().name.unwrap(), &message(name));
        } else {
            let Some(first_argument) = node.first_argument() else {
                return;
            };
            cx.add_offense(
                first_argument.source_range(),
                &message(first_argument.source()),
            );
        }
    }
}
