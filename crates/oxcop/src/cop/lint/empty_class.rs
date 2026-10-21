//! Port of `lib/rubocop/cop/lint/empty_class.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const CLASS_MSG: &str = "Empty class detected.";
const METACLASS_MSG: &str = "Empty metaclass detected.";

#[derive(Default)]
pub struct EmptyClass;

impl Cop for EmptyClass {
    fn name(&self) -> &'static str {
        "Lint/EmptyClass"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Class) {
            if !(body_or_allowed_comment_lines(cx, node) || node.parent_class().is_some()) {
                cx.add_offense(node.source_range(), CLASS_MSG);
            }
        } else if !body_or_allowed_comment_lines(cx, node) {
            cx.add_offense(node.source_range(), METACLASS_MSG);
        }
    }
}

fn body_or_allowed_comment_lines(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if node.body().is_some() {
        return true;
    }
    cx.cop_config_bool("AllowComments", false)
        && cx.processed_source.contains_comment(node.source_range())
}
