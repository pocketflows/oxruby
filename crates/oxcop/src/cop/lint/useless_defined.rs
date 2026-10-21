//! Port of `lib/rubocop/cop/lint/useless_defined.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct UselessDefined;

impl Cop for UselessDefined {
    fn name(&self) -> &'static str {
        "Lint/UselessDefined"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Defined]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(first_argument) = node.first_argument() else {
            return;
        };
        let type_ = match first_argument.ty() {
            NodeType::Str | NodeType::Dstr => "string",
            NodeType::Sym | NodeType::Dsym => "symbol",
            _ => return,
        };

        let message = format!(
            "Calling `defined?` with a {type_} argument will always return a truthy value."
        );
        cx.add_offense(node.source_range(), &message);
    }
}
