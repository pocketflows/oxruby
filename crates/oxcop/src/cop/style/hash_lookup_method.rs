//! Port of `lib/rubocop/cop/style/hash_lookup_method.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_receivers::allowed_receiver;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const BRACKET_MSG: &str = "Use `Hash#[]` instead of `Hash#fetch`.";
const FETCH_MSG: &str = "Use `Hash#fetch` instead of `Hash#[]`.";

#[derive(Default)]
pub struct HashLookupMethod {
    ignored: IgnoredNodes,
}

impl Cop for HashLookupMethod {
    fn name(&self) -> &'static str {
        "Style/HashLookupMethod"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]", "fetch"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_some_and(|r| allowed_receiver(cx, r)) {
            return;
        }
        if self.ignored.part_of(node) {
            return;
        }
        let style = style(cx);
        if offense_for_brackets(style, node) {
            self.ignored.ignore(node.first_argument().unwrap());
            cx.add_offense_with_correction(node.loc().selector.unwrap(), BRACKET_MSG, |corrector| {
                let key = node.first_argument().unwrap().source();
                corrector.replace(
                    node.loc().dot.unwrap().join(node.source_range().end()),
                    format!("[{key}]"),
                );
            });
        } else if offense_for_fetch(style, node) {
            self.ignored.ignore(node.first_argument().unwrap());
            cx.add_offense_with_correction(node.source_range(), FETCH_MSG, |corrector| {
                let key = node.first_argument().unwrap().source();
                if node.is(NodeType::Csend) {
                    corrector.replace(
                        node.loc().dot.unwrap().join(node.source_range().end()),
                        format!("&.fetch({key})"),
                    );
                } else {
                    corrector.replace(
                        node.loc().selector.unwrap().join(node.source_range().end()),
                        format!(".fetch({key})"),
                    );
                }
            });
        }
    }
}

fn offense_for_brackets(style: &str, node: Node<'_>) -> bool {
    style == "brackets"
        && node.receiver().is_some()
        && node.is_method("fetch")
        && node.arguments().len() == 1
        && !node.is_block_literal()
        && !node.is(NodeType::Csend)
}

fn offense_for_fetch(style: &str, node: Node<'_>) -> bool {
    style == "fetch" && node.is_method("[]") && node.arguments().len() == 1
}
