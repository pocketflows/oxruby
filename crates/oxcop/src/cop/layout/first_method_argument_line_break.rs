//! Port of `lib/rubocop/cop/layout/first_method_argument_line_break.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::first_element_line_break::check_method_line_break;
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "Add a line break before the first argument of a multi-line method argument list.";

#[derive(Default)]
pub struct FirstMethodArgumentLineBreak;

impl Cop for FirstMethodArgumentLineBreak {
    fn name(&self) -> &'static str {
        "Layout/FirstMethodArgumentLineBreak"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Super]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.method_name().is_some_and(|name| allowed_method(cx, name)) {
            return;
        }
        let mut args = node.arguments();
        if let Some(last_arg) = args.last().copied()
            && last_arg.is(NodeType::Hash)
            && !last_arg.has_braces()
        {
            args.pop();
            args.extend(last_arg.child_nodes());
        }
        let ignore_last = cx.cop_config_bool("AllowMultilineFinalElement", false);
        check_method_line_break(cx, MSG, node, &args, ignore_last);
    }
}
