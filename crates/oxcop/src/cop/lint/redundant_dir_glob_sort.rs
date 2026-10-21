//! Port of `lib/rubocop/cop/lint/redundant_dir_glob_sort.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove redundant `sort`.";
const GLOB_METHODS: &[&str] = &["glob", "[]"];

#[derive(Default)]
pub struct RedundantDirGlobSort;

impl Cop for RedundantDirGlobSort {
    fn name(&self) -> &'static str {
        "Lint/RedundantDirGlobSort"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["sort"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        if !dir_glob(receiver) {
            return;
        }
        if sort_with_comparator(node) || multiple_argument(receiver) {
            return;
        }

        let (Some(selector), dot) = (node.loc().selector, node.loc().dot) else {
            return;
        };
        cx.add_offense_with_correction(selector, MSG, |corrector| {
            corrector.remove(selector);
            if let Some(dot) = dot {
                corrector.remove(dot);
            }
        });
    }
}

fn dir_glob(receiver: Node<'_>) -> bool {
    let Some(receiver_receiver) = receiver.receiver() else {
        return false;
    };
    if !receiver_receiver.is(NodeType::Const) {
        return false;
    }
    if receiver_receiver.short_name() != Some("Dir") {
        return false;
    }
    receiver
        .method_name()
        .is_some_and(|m| GLOB_METHODS.contains(&m))
}

fn multiple_argument(glob_method: Node<'_>) -> bool {
    glob_method.arguments().len() >= 2
        || glob_method
            .first_argument()
            .is_some_and(|a| a.is(NodeType::Splat))
}

fn sort_with_comparator(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is_any_block())
        || node
            .last_argument()
            .is_some_and(|a| a.is(NodeType::BlockPass))
}
