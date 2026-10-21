//! Port of `lib/rubocop/cop/style/trailing_comma_in_hash_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::trailing_comma::check_literal;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct TrailingCommaInHashLiteral;

impl Cop for TrailingCommaInHashLiteral {
    fn name(&self) -> &'static str {
        "Style/TrailingCommaInHashLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        check_literal(cx, node, "item of %<article>s hash");
    }
}
