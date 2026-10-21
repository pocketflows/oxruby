//! Port of `lib/rubocop/cop/layout/space_in_lambda_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_REQUIRE_SPACE: &str = "Use a space between `->` and `(` in lambda literals.";
const MSG_REQUIRE_NO_SPACE: &str = "Do not use spaces between `->` and `(` in lambda literals.";

#[derive(Default)]
pub struct SpaceInLambdaLiteral;

impl Cop for SpaceInLambdaLiteral {
    fn name(&self) -> &'static str {
        "Layout/SpaceInLambdaLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["lambda"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !arrow_lambda_with_args(node) {
            return;
        }
        let style = style(cx);
        if style == "require_space" && !space_after_arrow(node) {
            let lambda_node = range_of_offense(node);
            let arguments = lambda_arguments(node).source_range();
            cx.add_offense_with_correction(lambda_node, MSG_REQUIRE_SPACE, |corrector| {
                corrector.insert_before(arguments, " ");
            });
        } else if style == "require_no_space" && space_after_arrow(node) {
            let space = space_after_arrow_range(node);
            cx.add_offense_with_correction(space, MSG_REQUIRE_NO_SPACE, |corrector| {
                corrector.remove(space);
            });
        }
    }
}

fn arrow_lambda_with_args(node: Node<'_>) -> bool {
    node.is_lambda_literal() && node.parent().is_some_and(|p| p.has_arguments())
}

fn space_after_arrow(lambda_node: Node<'_>) -> bool {
    !space_after_arrow_range(lambda_node).is_empty()
}

fn space_after_arrow_range(lambda_node: Node<'_>) -> Range {
    let parent = lambda_node.parent().unwrap();
    let arrow = parent.child(0).unwrap().source_range();
    let parentheses = parent.child(1).unwrap().source_range();
    arrow.end().join(parentheses.begin())
}

fn range_of_offense(node: Node<'_>) -> Range {
    let parent = node.parent().unwrap();
    range_between(
        parent.source_range().begin_pos,
        parent.arguments_node().unwrap().source_range().end_pos,
    )
}

fn lambda_arguments(node: Node<'_>) -> Node<'_> {
    node.parent().unwrap().child(1).unwrap()
}
