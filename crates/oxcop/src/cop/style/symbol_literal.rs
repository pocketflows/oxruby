//! Port of `lib/rubocop/cop/style/symbol_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Do not use strings for word-like symbol literals.";

#[derive(Default)]
pub struct SymbolLiteral;

impl Cop for SymbolLiteral {
    fn name(&self) -> &'static str {
        "Style/SymbolLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Sym]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if regex!(r#"\A:["'][A-Za-z_][A-Za-z0-9_]*["']\z"#)
            .find(node.source())
            .is_none()
        {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), node.source().replace(['\'', '"'], ""));
        });
    }
}
