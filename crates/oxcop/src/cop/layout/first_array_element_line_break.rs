//! Port of `lib/rubocop/cop/layout/first_array_element_line_break.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::first_element_line_break::check_children_line_break;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Add a line break before the first element of a multi-line array.";

#[derive(Default)]
pub struct FirstArrayElementLineBreak;

impl Cop for FirstArrayElementLineBreak {
    fn name(&self) -> &'static str {
        "Layout/FirstArrayElementLineBreak"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.loc().begin.is_none() && !assignment_on_same_line(cx, node) {
            return;
        }
        if cx.cop_config_bool("AllowImplicitArrayLiterals", false) && !node.is_bracketed() {
            return;
        }
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_children_line_break(cx, MSG, node, &children, None, ignore_last);
    }
}

fn assignment_on_same_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let line = cx.processed_source.source_line(node.first_line());
    let source: String = line.chars().take(node.column()).collect();
    regex!(r"\s*=\s*$").find(&source).is_some()
}
