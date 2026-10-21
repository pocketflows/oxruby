//! Port of `lib/rubocop/cop/mixin/empty_parameter.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::NodePattern;
use crate::cop::{Corrector, Investigation};

static EMPTY_ARGUMENTS: NodePattern = NodePattern::new("(block _ $(args) _)");

/// `check(node)`: registers `message` on empty but delimited block
/// arguments (`||`, `()`), correcting with `autocorrect(corrector, args)`.
pub fn check<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    message: &str,
    autocorrect: impl FnOnce(&mut Corrector, Node<'a>),
) {
    let Some(captures) = EMPTY_ARGUMENTS.match_node(node) else {
        return;
    };
    let args = captures[0].node().unwrap();
    // `empty_and_without_delimiters?`
    if args.loc().expression.is_none() {
        return;
    }
    cx.add_offense_with_correction(args.source_range(), message, |corrector| {
        autocorrect(corrector, args);
    });
}
