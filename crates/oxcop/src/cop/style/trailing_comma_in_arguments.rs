//! Port of `lib/rubocop/cop/style/trailing_comma_in_arguments.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::trailing_comma::check;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct TrailingCommaInArguments;

impl Cop for TrailingCommaInArguments {
    fn name(&self) -> &'static str {
        "Style/TrailingCommaInArguments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Layout/HeredocArgumentClosingParenthesis"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let arguments = node.arguments();
        let Some(last) = arguments.last() else {
            return;
        };
        if !(node.is_parenthesized() || node.is_method("[]")) {
            return;
        }
        check(
            cx,
            node,
            &arguments,
            "parameter of %<article>s method call",
            last.source_range().end_pos,
            node.source_range().end_pos,
        );
    }
}
