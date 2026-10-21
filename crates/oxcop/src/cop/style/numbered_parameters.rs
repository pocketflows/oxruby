//! Port of `lib/rubocop/cop/style/numbered_parameters.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG_DISALLOW: &str = "Avoid using numbered parameters.";
const MSG_MULTI_LINE: &str = "Avoid using numbered parameters for multi-line blocks.";

#[derive(Default)]
pub struct NumberedParameters;

impl Cop for NumberedParameters {
    fn name(&self) -> &'static str {
        "Style/NumberedParameters"
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Numblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if style(cx) == "disallow" {
            cx.add_offense(node.source_range(), MSG_DISALLOW);
        } else if node.is_multiline() {
            cx.add_offense(node.source_range(), MSG_MULTI_LINE);
        }
    }
}
