//! Port of `lib/rubocop/cop/style/redundant_each.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Remove redundant `each`.";
const MSG_WITH_INDEX: &str = "Use `with_index` to remove redundant `each`.";
const MSG_WITH_OBJECT: &str = "Use `with_object` to remove redundant `each`.";

const RESTRICT_ON_SEND: &[&str] = &["each", "each_with_index", "each_with_object"];

#[derive(Default)]
pub struct RedundantEach;

impl Cop for RedundantEach {
    fn name(&self) -> &'static str {
        "Style/RedundantEach"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(redundant_node) = redundant_each_method(node) else {
            return;
        };
        let range = range(node);
        let message = match node.method_name() {
            Some("each") => MSG,
            Some("each_with_index") => MSG_WITH_INDEX,
            _ => MSG_WITH_OBJECT,
        };
        cx.add_offense_with_correction(range, message, |corrector| match node.method_name() {
            Some("each") => {
                corrector.remove(range);
                if redundant_node.is_method("each_with_index") {
                    corrector.replace(redundant_node.loc().selector.unwrap(), "each.with_index");
                } else if redundant_node.is_method("each_with_object") {
                    corrector.replace(redundant_node.loc().selector.unwrap(), "each.with_object");
                }
            }
            Some("each_with_index") => {
                corrector.replace(node.loc().selector.unwrap(), "with_index");
            }
            Some("each_with_object") => {
                corrector.replace(node.loc().selector.unwrap(), "with_object");
            }
            _ => {}
        });
    }
}

fn block_pass_last(node: Node<'_>) -> bool {
    node.last_argument()
        .is_some_and(|a| a.is(NodeType::BlockPass))
}

fn redundant_each_method(node: Node<'_>) -> Option<Node<'_>> {
    if block_pass_last(node) {
        return None;
    }
    if node.is_method("each") && !node.parent().is_some_and(|p| p.is_any_block()) {
        let ancestor_node = node.ancestors().filter(|a| a.is_call()).find(|ancestor| {
            ancestor.receiver().is_some_and(|r| r.same_as(node))
                && ancestor.method_name().is_some_and(|m| {
                    RESTRICT_ON_SEND.contains(&m) || m == "reverse_each"
                })
        });
        if ancestor_node.is_some() {
            return ancestor_node;
        }
    }
    let prev_method = node.child(0)?;
    if !prev_method.is_call()
        || prev_method.parent().is_some_and(|p| p.is_any_block())
        || block_pass_last(prev_method)
    {
        return None;
    }
    let detected = !node.is_method("each")
        && prev_method
            .method_name()
            .is_some_and(|m| m.starts_with("each_"));
    (detected || prev_method.is_method("reverse_each")).then_some(prev_method)
}

fn range(node: Node<'_>) -> Range {
    let selector = node.selector().unwrap();
    if !node.is_method("each") {
        return selector;
    }
    match node.parent() {
        Some(parent) if parent.is_call() => parent.loc().dot.map_or(selector, |dot| selector.join(dot)),
        _ => node.loc().dot.unwrap().join(selector),
    }
}
