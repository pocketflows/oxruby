//! Port of `lib/rubocop/cop/mixin/rational_literal.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::NodePattern;

static RATIONAL_LITERAL: NodePattern = NodePattern::new("(send (int _) :/ (rational _))");

/// `rational_literal?(node)`
pub fn rational_literal(node: Node<'_>) -> bool {
    RATIONAL_LITERAL.is_match(node)
}
