//! Port of `lib/rubocop/cop/metrics/block_length.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::{allowed_method, allowed_methods};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::code_length::check_code_length;
use crate::cop::{Cop, Investigation};

const LABEL: &str = "Block";

#[derive(Default)]
pub struct BlockLength;

impl Cop for BlockLength {
    fn name(&self) -> &'static str {
        "Metrics/BlockLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method_name = node.method_name().unwrap_or("");
        if allowed_method(cx, method_name) || matches_allowed_pattern(cx, method_name) {
            return;
        }
        if method_receiver_excluded(cx, node) {
            return;
        }
        if node.is_class_constructor() {
            return;
        }
        check_code_length(cx, node, LABEL);
    }
}

fn method_receiver_excluded(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let node_receiver: Option<String> = node
        .receiver()
        .map(|r| r.source().chars().filter(|c| !is_space(*c)).collect());
    let node_method = node.method_name().unwrap_or("");
    allowed_methods(cx).iter().any(|config| {
        let mut parts = config.split('.');
        let first = parts.next().unwrap_or("");
        let (receiver, method) = match parts.next() {
            Some(method) => (Some(first.to_owned()), method),
            None => (node_receiver.clone(), first),
        };
        method == node_method && receiver == node_receiver
    })
}

/// `\s` in Ruby.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}
