//! Port of `lib/rubocop/cop/mixin/percent_literal.rb`. Cops call
//! [`process`] from their `on_array`/`on_str`/... callbacks and run their
//! `on_percent_literal` when it returns true.

use crate::ast::Node;

/// `percent_literal?(node)`: `loc.begin` starts with `%`.
pub fn percent_literal(node: Node<'_>) -> bool {
    begin_source(node).is_some_and(|s| s.starts_with('%'))
}

/// `process(node, *types)`: whether `on_percent_literal(node)` runs, i.e.
/// the node is a percent literal of one of `types` (`%w`, `%I`, ...).
pub fn process(node: Node<'_>, types: &[&str]) -> bool {
    percent_literal(node) && types.contains(&percent_type(node))
}

/// `begin_source(node)`.
pub fn begin_source<'a>(node: Node<'a>) -> Option<&'a str> {
    node.loc().begin.map(|begin| node.range_source(begin))
}

/// `type(node)`: the opening delimiter without its last character (`%w`).
pub fn percent_type<'a>(node: Node<'a>) -> &'a str {
    let source = begin_source(node).unwrap_or_default();
    match source.char_indices().last() {
        Some((i, _)) => &source[..i],
        None => source,
    }
}
