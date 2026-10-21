//! Port of `lib/rubocop/cop/mixin/gem_declaration.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::NodePattern;

static GEM_DECLARATION: NodePattern = NodePattern::new("(send nil? :gem str ...)");

/// `gem_declaration?(node)`.
pub fn gem_declaration(node: Node<'_>) -> bool {
    GEM_DECLARATION.is_match(node)
}
