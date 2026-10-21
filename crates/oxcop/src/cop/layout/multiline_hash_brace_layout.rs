//! Port of `lib/rubocop/cop/layout/multiline_hash_brace_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_literal_brace_layout::{Messages, check_brace_layout};
use crate::cop::{Cop, Investigation};

const MESSAGES: Messages = Messages {
    same_line: "Closing hash brace must be on the same line as the last hash element when opening brace is on the same line as the first hash element.",
    new_line: "Closing hash brace must be on the line after the last hash element when opening brace is on a separate line from the first hash element.",
    always_new_line: "Closing hash brace must be on the line after the last hash element.",
    always_same_line: "Closing hash brace must be on the same line as the last hash element.",
};

#[derive(Default)]
pub struct MultilineHashBraceLayout;

impl Cop for MultilineHashBraceLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineHashBraceLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        check_brace_layout(cx, node, &children, &MESSAGES);
    }
}
