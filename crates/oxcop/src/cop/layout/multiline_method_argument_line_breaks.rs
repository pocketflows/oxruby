//! Port of `lib/rubocop/cop/layout/multiline_method_argument_line_breaks.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_element_line_breaks::check_line_breaks;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Each argument in a multi-line method call must start on a separate line.";

#[derive(Default)]
pub struct MultilineMethodArgumentLineBreaks;

impl Cop for MultilineMethodArgumentLineBreaks {
    fn name(&self) -> &'static str {
        "Layout/MultilineMethodArgumentLineBreaks"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_method("[]=") {
            return;
        }
        let mut children = node.arguments();
        if let Some(last_arg) = children.last().copied()
            && last_arg.is(NodeType::Hash)
            && !last_arg.has_braces()
        {
            children.pop();
            children.extend(last_arg.child_nodes());
        }
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_line_breaks(cx, &children, ignore_last, MSG);
    }
}
