//! Port of `lib/rubocop/cop/layout/multiline_method_parameter_line_breaks.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::multiline_element_line_breaks::check_line_breaks;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Each parameter in a multi-line method definition must start on a separate line.";

#[derive(Default)]
pub struct MultilineMethodParameterLineBreaks;

impl Cop for MultilineMethodParameterLineBreaks {
    fn name(&self) -> &'static str {
        "Layout/MultilineMethodParameterLineBreaks"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let children = node.arguments();
        if children.is_empty() {
            return;
        }
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_line_breaks(cx, &children, ignore_last, MSG);
    }
}
