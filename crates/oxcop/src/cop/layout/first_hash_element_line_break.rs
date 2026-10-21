//! Port of `lib/rubocop/cop/layout/first_hash_element_line_break.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::first_element_line_break::check_children_line_break;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Add a line break before the first element of a multi-line hash.";

#[derive(Default)]
pub struct FirstHashElementLineBreak;

impl Cop for FirstHashElementLineBreak {
    fn name(&self) -> &'static str {
        "Layout/FirstHashElementLineBreak"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.loc().begin.is_none() {
            return;
        }
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_children_line_break(cx, MSG, node, &children, None, ignore_last);
    }
}
