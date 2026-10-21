//! Port of `lib/rubocop/cop/lint/empty_when.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::{comments_contain_disables, contains_comments};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid `when` branches without a body.";

#[derive(Default)]
pub struct EmptyWhen;

impl Cop for EmptyWhen {
    fn name(&self) -> &'static str {
        "Lint/EmptyWhen"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for when_node in node.when_branches() {
            if when_node.body().is_some() {
                continue;
            }
            if allow_comments(cx, when_node) {
                continue;
            }
            cx.add_offense(when_node.source_range(), MSG);
        }
    }
}

fn allow_comments(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowComments", false)
        && contains_comments(cx.processed_source, node)
        && !comments_contain_disables(cx, node, cx.cop_name())
}
