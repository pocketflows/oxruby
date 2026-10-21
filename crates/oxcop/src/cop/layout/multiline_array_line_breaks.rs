//! Port of `lib/rubocop/cop/layout/multiline_array_line_breaks.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_element_line_breaks::check_line_breaks;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Each item in a multi-line array must start on a separate line.";

#[derive(Default)]
pub struct MultilineArrayLineBreaks;

impl Cop for MultilineArrayLineBreaks {
    fn name(&self) -> &'static str {
        "Layout/MultilineArrayLineBreaks"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_line_breaks(cx, &children, ignore_last, MSG);
    }
}
