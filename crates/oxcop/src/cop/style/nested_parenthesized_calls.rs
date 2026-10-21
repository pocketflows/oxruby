//! Port of `lib/rubocop/cop/style/nested_parenthesized_calls.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct NestedParenthesizedCalls;

impl Cop for NestedParenthesizedCalls {
    fn name(&self) -> &'static str {
        "Style/NestedParenthesizedCalls"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/MethodCallWithArgsParentheses"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_parenthesized() {
            return;
        }
        let src = cx.processed_source.buffer_source();
        let nested: Vec<Node<'a>> = node.child_nodes().filter(|c| c.is_call()).collect();
        for nested in nested {
            if allowed_omission(cx, nested) {
                continue;
            }
            let message = format!("Add parentheses to nested method call `{}`.", nested.source());
            cx.add_offense_with_correction(nested.source_range(), &message, |corrector| {
                let first_arg = nested.first_argument().unwrap().source_range();
                let last_arg = nested.last_argument().unwrap().source_range();
                let leading_space = range_with_surrounding_space(
                    src,
                    first_arg.begin(),
                    SurroundingSpace {
                        side: Side::Left,
                        whitespace: true,
                        continuations: true,
                        ..SurroundingSpace::default()
                    },
                );
                corrector.replace(leading_space, "(");
                corrector.insert_after(last_arg, ")");
            });
        }
    }
}

fn allowed_omission(cx: &Investigation<'_, '_>, send_node: Node<'_>) -> bool {
    !send_node.has_arguments()
        || send_node.is_parenthesized()
        || send_node.is_setter_method()
        || send_node.is_operator_method()
        || allowed(cx, send_node)
}

fn allowed(cx: &Investigation<'_, '_>, send_node: Node<'_>) -> bool {
    send_node.parent().is_some_and(|p| p.arguments().len() == 1)
        && allowed_method(cx, send_node.method_name().unwrap_or(""))
        && send_node.arguments().len() == 1
}
