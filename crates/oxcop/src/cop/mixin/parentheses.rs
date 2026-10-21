//! Port of `lib/rubocop/cop/mixin/parentheses.rb`.

use crate::ast::Node;

/// `parens_required?(node)`: a letter right before or after the node.
pub fn parens_required(node: Node<'_>) -> bool {
    let range = node.source_range();
    let source = node.ast().source().as_bytes();
    let before = range
        .begin_pos
        .checked_sub(1)
        .and_then(|i| source.get(i))
        .is_some_and(u8::is_ascii_lowercase);
    let after = source
        .get(range.end_pos)
        .is_some_and(u8::is_ascii_lowercase);
    before || after
}
