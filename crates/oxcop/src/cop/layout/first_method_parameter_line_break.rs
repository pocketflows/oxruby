//! Port of `lib/rubocop/cop/layout/first_method_parameter_line_break.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::first_element_line_break::check_method_line_break;
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "Add a line break before the first parameter of a multi-line method parameter list.";

#[derive(Default)]
pub struct FirstMethodParameterLineBreak;

impl Cop for FirstMethodParameterLineBreak {
    fn name(&self) -> &'static str {
        "Layout/FirstMethodParameterLineBreak"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_method_line_break(cx, MSG, node, &node.arguments(), ignore_last);
    }
}
