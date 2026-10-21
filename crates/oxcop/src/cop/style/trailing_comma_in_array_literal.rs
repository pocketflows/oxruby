//! Port of `lib/rubocop/cop/style/trailing_comma_in_array_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::trailing_comma::check_literal;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct TrailingCommaInArrayLiteral;

impl Cop for TrailingCommaInArrayLiteral {
    fn name(&self) -> &'static str {
        "Style/TrailingCommaInArrayLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_square_brackets() {
            return;
        }
        check_literal(cx, node, "item of %<article>s array");
    }
}
