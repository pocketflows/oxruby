//! Port of `lib/rubocop/cop/lint/unreachable_pattern_branch.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Unreachable `in` pattern branch detected.";
const MSG_ELSE: &str = "Unreachable `else` branch detected.";

#[derive(Default)]
pub struct UnreachablePatternBranch;

impl Cop for UnreachablePatternBranch {
    fn name(&self) -> &'static str {
        "Lint/UnreachablePatternBranch"
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::CaseMatch]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
        let mut catch_all_found = false;
        for in_pattern_node in case_node.when_branches() {
            if catch_all_found {
                cx.add_offense(in_pattern_node.source_range(), MSG);
                continue;
            }

            let guard = in_pattern_node.child(1);
            if in_pattern_node.child(0).is_some_and(catch_all_pattern) && guard.is_none() {
                catch_all_found = true;
            }
        }

        if !catch_all_found {
            return;
        }
        if let Some(else_) = case_node.loc().else_ {
            cx.add_offense(else_, MSG_ELSE);
        }
    }
}

fn catch_all_pattern(pattern: Node<'_>) -> bool {
    match pattern.ty() {
        NodeType::MatchVar => true,
        NodeType::MatchAs | NodeType::Begin => pattern.child(0).is_some_and(catch_all_pattern),
        NodeType::MatchAlt => pattern.child_nodes().any(catch_all_pattern),
        _ => false,
    }
}
