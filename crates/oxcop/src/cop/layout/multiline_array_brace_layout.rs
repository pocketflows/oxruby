//! Port of `lib/rubocop/cop/layout/multiline_array_brace_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_literal_brace_layout::{Messages, check_brace_layout};
use crate::cop::{Cop, Investigation};

const MESSAGES: Messages = Messages {
    same_line: "The closing array brace must be on the same line as the last array element when the opening brace is on the same line as the first array element.",
    new_line: "The closing array brace must be on the line after the last array element when the opening brace is on a separate line from the first array element.",
    always_new_line: "The closing array brace must be on the line after the last array element.",
    always_same_line: "The closing array brace must be on the same line as the last array element.",
};

#[derive(Default)]
pub struct MultilineArrayBraceLayout;

impl Cop for MultilineArrayBraceLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineArrayBraceLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        check_brace_layout(cx, node, &children, &MESSAGES);
    }
}
