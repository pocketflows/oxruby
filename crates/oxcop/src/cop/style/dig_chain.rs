//! Port of `lib/rubocop/cop/style/dig_chain.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::comments_in_range;
use crate::cop::mixin::dig_help::dig;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct DigChain {
    ignored: IgnoredNodes,
}

impl Cop for DigChain {
    fn name(&self) -> &'static str {
        "Style/DigChain"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["dig"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.contains(node) {
            return;
        }
        if node.loc().dot.is_none() || !dig(node) {
            return;
        }
        let Some((range, arguments)) = self.inspect_chain(node) else {
            return;
        };
        if invalid_arguments(&arguments) {
            return;
        }
        register_offense(cx, node, range, &arguments);
    }
}

impl DigChain {
    fn inspect_chain<'a>(&mut self, node: Node<'a>) -> Option<(Range, Vec<Node<'a>>)> {
        let mut arguments = node.arguments();
        let end_range = node.source_range().end();
        let mut begin_range = None;
        let mut node = node;
        while let Some(receiver) = node.receiver().filter(|r| dig(*r)) {
            node = receiver;
            begin_range = node.loc().selector;
            let mut args = node.arguments();
            args.extend(arguments);
            arguments = args;
            self.ignored.ignore(node);
        }
        Some((begin_range?.join(end_range), arguments))
    }
}

fn invalid_arguments(arguments: &[Node<'_>]) -> bool {
    // A `(...)` forwarded argument cannot be followed by more arguments.
    if arguments.is_empty() {
        return false;
    }
    arguments
        .iter()
        .position(|a| a.is(NodeType::ForwardedArgs))
        .is_some_and(|index| index < arguments.len() - 1)
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    range: Range,
    arguments: &[Node<'_>],
) {
    let arguments: Vec<&str> = arguments.iter().map(|a| a.source()).collect();
    let replacement = format!("dig({})", arguments.join(", "));
    let message = format!("Use `{replacement}` instead of chaining.");
    let ps = cx.processed_source;
    cx.add_offense_with_correction(range, &message, |corrector| {
        corrector.replace(range, &replacement);
        for comment in comments_in_range(ps, node).iter().rev() {
            if comment.range.begin_pos >= range.end_pos {
                continue;
            }
            corrector.insert_before(node.source_range(), format!("{}\n", comment.text));
        }
    });
}
