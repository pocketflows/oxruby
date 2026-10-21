//! Port of `lib/rubocop/cop/mixin/on_normal_if_unless.rb`: `on_if` calls
//! the cop's `on_normal_if_unless` for `if`/`unless` nodes that are neither
//! modifiers nor ternaries.

use crate::ast::Node;

/// Whether `on_if` passes `node` on to `on_normal_if_unless`.
pub fn normal_if_unless(node: Node<'_>) -> bool {
    !(node.is_modifier_form() || node.is_ternary())
}
