//! Port of `lib/rubocop/cop/lint/trailing_comma_in_attribute_declaration.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid leaving a trailing comma in attribute declarations.";

#[derive(Default)]
pub struct TrailingCommaInAttributeDeclaration;

impl Cop for TrailingCommaInAttributeDeclaration {
    fn name(&self) -> &'static str {
        "Lint/TrailingCommaInAttributeDeclaration"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["attr_reader", "attr_writer", "attr_accessor", "attr"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.attribute_accessor().is_none()
            || !node.last_argument().is_some_and(|a| a.is_any_def())
        {
            return;
        }
        if node.arguments().len() <= 1 {
            return;
        }

        let trailing_comma = trailing_comma_range(cx, node);
        cx.add_offense_with_correction(trailing_comma, MSG, |corrector| {
            corrector.remove(trailing_comma);
        });
    }
}

fn trailing_comma_range(cx: &Investigation<'_, '_>, node: Node<'_>) -> Range {
    let arguments = node.arguments();
    range_with_surrounding_space(
        cx.processed_source.buffer_source(),
        arguments[arguments.len() - 2].source_range(),
        SurroundingSpace::side(Side::Right),
    )
    .end()
    .resize(1)
}
