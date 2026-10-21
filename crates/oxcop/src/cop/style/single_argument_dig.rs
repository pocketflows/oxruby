//! Port of `lib/rubocop/cop/style/single_argument_dig.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::dig_help::{dig, dig_chain_enabled, single_argument_dig};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const IGNORED_ARGUMENT_TYPES: &[NodeType] = &[
    NodeType::BlockPass,
    NodeType::ForwardedRestarg,
    NodeType::ForwardedArgs,
    NodeType::Hash,
];

#[derive(Default)]
pub struct SingleArgumentDig {
    ignored: IgnoredNodes,
}

impl Cop for SingleArgumentDig {
    fn name(&self) -> &'static str {
        "Style/SingleArgumentDig"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["dig"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(expression) = single_argument_dig(node) else {
            return;
        };
        if expression.is_any(IGNORED_ARGUMENT_TYPES) || ignore_dig_chain(cx, node) {
            return;
        }
        let receiver = receiver.source();
        let argument = expression.source();
        let message = format!(
            "Use `{receiver}[{argument}]` instead of `{}`.",
            node.source()
        );
        let part_of_ignored = self.ignored.part_of(node);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if part_of_ignored {
                return;
            }
            corrector.replace(node.source_range(), format!("{receiver}[{argument}]"));
        });
        self.ignored.ignore(node);
    }
}

fn ignore_dig_chain(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    dig_chain_enabled(cx)
        && (node.receiver().is_some_and(dig) || node.parent().is_some_and(dig))
}
