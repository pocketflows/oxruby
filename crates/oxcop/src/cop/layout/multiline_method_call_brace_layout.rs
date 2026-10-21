//! Port of `lib/rubocop/cop/layout/multiline_method_call_brace_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_literal_brace_layout::{Messages, check_brace_layout};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

const MESSAGES: Messages = Messages {
    same_line: "Closing method call brace must be on the same line as the last argument when opening brace is on the same line as the first argument.",
    new_line: "Closing method call brace must be on the line after the last argument when opening brace is on a separate line from the first argument.",
    always_new_line: "Closing method call brace must be on the line after the last argument.",
    always_same_line: "Closing method call brace must be on the same line as the last argument.",
};

#[derive(Default)]
pub struct MultilineMethodCallBraceLayout;

impl Cop for MultilineMethodCallBraceLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineMethodCallBraceLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if single_line_ignoring_receiver(cx, node) {
            return;
        }
        let children = node.arguments();
        check_brace_layout(cx, node, &children, &MESSAGES);
    }
}

fn single_line_ignoring_receiver(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let (Some(begin), Some(end)) = (node.loc().begin, node.loc().end) else {
        return false;
    };
    same_line(cx.processed_source, begin, end)
}
