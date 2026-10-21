//! Port of `lib/rubocop/cop/layout/multiline_hash_key_line_breaks.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_element_line_breaks::check_line_breaks;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Each key in a multi-line hash must start on a separate line.";

#[derive(Default)]
pub struct MultilineHashKeyLineBreaks;

impl Cop for MultilineHashKeyLineBreaks {
    fn name(&self) -> &'static str {
        "Layout/MultilineHashKeyLineBreaks"
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
        check_line_breaks(cx, &children, ignore_last, MSG);
    }
}
