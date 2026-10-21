//! Port of `lib/rubocop/cop/mixin/dig_help.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::NodePattern;
use crate::cop::Investigation;

static DIG: NodePattern = NodePattern::new("(call _ :dig !{hash block_pass}+)");

static SINGLE_ARGUMENT_DIG: NodePattern = NodePattern::new("(send _ :dig $!splat)");

/// `dig?(node)`.
pub fn dig(node: Node<'_>) -> bool {
    DIG.is_match(node)
}

/// `single_argument_dig?(node)`: the single argument.
pub fn single_argument_dig(node: Node<'_>) -> Option<Node<'_>> {
    SINGLE_ARGUMENT_DIG
        .match_node(node)
        .and_then(|captures| captures[0].node())
}

/// `dig_chain_enabled?`
pub fn dig_chain_enabled(cx: &Investigation<'_, '_>) -> bool {
    cx.config.cop_enabled("Style/DigChain")
}
