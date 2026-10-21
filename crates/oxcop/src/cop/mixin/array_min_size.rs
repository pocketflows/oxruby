//! Port of `lib/rubocop/cop/mixin/array_min_size.rb`.
//!
//! `array_style_detected` only feeds `config_to_allow_offenses` (used by
//! `--auto-gen-config`), so it isn't ported.

use crate::ast::Node;
use crate::cop::Investigation;

/// `below_array_length?(node)`.
pub fn below_array_length(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    (node.child_nodes().count() as i64) < min_size_config(cx)
}

/// `min_size_config`.
pub fn min_size_config(cx: &Investigation<'_, '_>) -> i64 {
    cx.cop_config_int("MinSize").unwrap_or(0)
}
