//! Port of `lib/rubocop/cop/mixin/def_node.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::NodePattern;
use crate::cop::mixin::visibility_help::node_visibility;

static NON_PUBLIC_MODIFIER: NodePattern =
    NodePattern::new("(send nil? {:private :protected :private_class_method} (any_def ...))");

/// `non_public?(node)`.
pub fn non_public(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| NON_PUBLIC_MODIFIER.is_match(parent))
        || preceding_non_public_modifier(node)
}

/// `preceding_non_public_modifier?(node)`.
pub fn preceding_non_public_modifier(node: Node<'_>) -> bool {
    node_visibility(node) != "public"
}
