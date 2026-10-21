//! Port of `lib/rubocop/cop/layout/multiline_method_definition_brace_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_literal_brace_layout::{Messages, check_brace_layout};
use crate::cop::{Cop, Investigation};

const MESSAGES: Messages = Messages {
    same_line: "Closing method definition brace must be on the same line as the last parameter when opening brace is on the same line as the first parameter.",
    new_line: "Closing method definition brace must be on the line after the last parameter when opening brace is on a separate line from the first parameter.",
    always_new_line: "Closing method definition brace must be on the line after the last parameter.",
    always_same_line: "Closing method definition brace must be on the same line as the last parameter.",
};

#[derive(Default)]
pub struct MultilineMethodDefinitionBraceLayout;

impl Cop for MultilineMethodDefinitionBraceLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineMethodDefinitionBraceLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(args) = node.arguments_node() else {
            return;
        };
        let children: Vec<Node<'a>> = args.child_nodes().collect();
        check_brace_layout(cx, args, &children, &MESSAGES);
    }
}
