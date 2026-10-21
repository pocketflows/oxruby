//! Port of `lib/rubocop/cop/style/send.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer `Object#__send__` or `Object#public_send` to `send`.";

#[derive(Default)]
pub struct Send;

impl Cop for Send {
    fn name(&self) -> &'static str {
        "Style/Send"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["send"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_arguments() {
            return;
        }
        if let Some(selector) = node.loc().selector {
            cx.add_offense(selector, MSG);
        }
    }
}
