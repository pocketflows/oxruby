//! Port of `lib/rubocop/cop/layout/space_before_first_arg.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::preceding_following_alignment::{
    PrecedingFollowingAlignment, allow_for_alignment,
};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Put one space between the method name and the first argument.";

#[derive(Default)]
pub struct SpaceBeforeFirstArg {
    alignment: PrecedingFollowingAlignment,
}

impl Cop for SpaceBeforeFirstArg {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeFirstArg"
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
        if !regular_method_call_with_arguments(node) {
            return;
        }
        if node.is_parenthesized() {
            return;
        }
        let first_arg = node.first_argument().unwrap().source_range();
        let first_arg_with_space = range_with_surrounding_space(
            cx.processed_source.buffer_source(),
            first_arg,
            SurroundingSpace::side(Side::Left),
        );
        let space = range_between(first_arg_with_space.begin_pos, first_arg.begin_pos);
        if space.len() == 1 {
            return;
        }
        if !self.expect_params_after_method_name(cx, node) {
            return;
        }
        cx.add_offense_with_correction(space, MSG, |corrector| corrector.replace(space, " "));
    }
}

impl SpaceBeforeFirstArg {
    fn expect_params_after_method_name(&mut self, cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
        if no_space_between_method_name_and_first_argument(node) {
            return true;
        }
        let first_arg = node.first_argument().unwrap();
        same_line(cx.processed_source, first_arg.source_range(), node.source_range())
            && !(allow_for_alignment(cx)
                && self
                    .alignment
                    .aligned_with_something(cx.processed_source, first_arg.source_range()))
    }
}

fn regular_method_call_with_arguments(node: Node<'_>) -> bool {
    node.has_arguments() && !node.is_operator_method() && !node.is_setter_method()
}

fn no_space_between_method_name_and_first_argument(node: Node<'_>) -> bool {
    let Some(selector) = node.loc().selector else {
        return false;
    };
    selector.end_pos == node.first_argument().unwrap().source_range().begin_pos
}
