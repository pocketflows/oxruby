//! Port of `lib/rubocop/cop/style/hash_fetch_chain.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

static DIGGABLE: NodePattern = NodePattern::new(
    "(call _ :fetch $_arg {nil (hash) (send (const {nil? cbase} :Hash) :new)})",
);

#[derive(Default)]
pub struct HashFetchChain {
    ignored: IgnoredNodes,
}

impl Cop for HashFetchChain {
    fn name(&self) -> &'static str {
        "Style/HashFetchChain"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.3)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["fetch"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        // `ignored_node?` compares nodes by identity, so nodes of an earlier
        // source never match.
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.contains(node) || last_fetch_non_nil(node) {
            return;
        }
        let (last_replaceable_node, arguments) = self.inspect_chain(node);
        let Some(last_replaceable_node) = last_replaceable_node else {
            return;
        };
        if arguments.len() <= 1 {
            return;
        }
        let range = last_replaceable_node
            .loc()
            .selector
            .unwrap()
            .join(node.loc().end.unwrap());
        let values: Vec<&str> = arguments.iter().map(|a| a.source()).collect();
        let replacement = format!("dig({})", values.join(", "));
        let message = format!("Use `{replacement}` instead.");
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &replacement);
        });
    }
}

/// When chaining `fetch` methods, `fetch(x, {})` is acceptable within the
/// chain, as long as the last method in the chain has a `nil` default.
fn last_fetch_non_nil(node: Node<'_>) -> bool {
    if !node.is_method("fetch") {
        return false;
    }
    !node.last_argument().is_some_and(|a| a.is(NodeType::Nil))
}

impl HashFetchChain {
    fn inspect_chain<'a>(&mut self, node: Node<'a>) -> (Option<Node<'a>>, Vec<Node<'a>>) {
        let mut arguments = Vec::new();
        let mut last_replaceable_node = None;
        let mut current = Some(node);
        while let Some(node) = current {
            let Some(arg) = DIGGABLE.match_node(node).and_then(|c| c[0].node()) else {
                break;
            };
            arguments.insert(0, arg);
            self.ignored.ignore(node);
            last_replaceable_node = Some(node);
            current = node.receiver();
        }
        (last_replaceable_node, arguments)
    }
}
