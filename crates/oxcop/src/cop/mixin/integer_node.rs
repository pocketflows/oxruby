//! Port of `lib/rubocop/cop/mixin/integer_node.rb`.

use crate::ast::Node;

/// `integer_part(node)`: the source without a sign, up to `e`, `E` or `.`.
pub fn integer_part<'a>(node: Node<'a>) -> &'a str {
    let source = node.source();
    let source = source.strip_prefix(['+', '-']).unwrap_or(source);
    source.split(['e', 'E', '.']).next().unwrap_or("")
}
