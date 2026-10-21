//! Port of `lib/rubocop/cop/lint/empty_in_pattern.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::{comments_contain_disables, contains_comments};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid `in` branches without a body.";

#[derive(Default)]
pub struct EmptyInPattern;

impl Cop for EmptyInPattern {
    fn name(&self) -> &'static str {
        "Lint/EmptyInPattern"
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::CaseMatch]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for branch in node.when_branches() {
            if branch.body().is_some() {
                continue;
            }
            if allow_comments(cx, branch) {
                continue;
            }
            cx.add_offense(branch.source_range(), MSG);
        }
    }
}

fn allow_comments(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowComments", false)
        && contains_comments(cx.processed_source, node)
        && !comments_contain_disables(cx, node, cx.cop_name())
}
