//! Port of `lib/rubocop/cop/lint/boolean_symbol.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

static BOOLEAN_SYMBOL: NodePattern = NodePattern::new("(sym {:true :false})");

#[derive(Default)]
pub struct BooleanSymbol;

impl Cop for BooleanSymbol {
    fn name(&self) -> &'static str {
        "Lint/BooleanSymbol"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Sym]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !BOOLEAN_SYMBOL.is_match(node) {
            return;
        }
        let parent = node.parent();
        if parent.is_some_and(|p| p.is(NodeType::Array) && p.is_percent_literal_of("symbol")) {
            return;
        }
        let message = format!(
            "Symbol with a boolean name - you probably meant to use `{}`.",
            node.sym_value().unwrap()
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            autocorrect(corrector, node);
        });
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    let mut boolean_literal = node.source().replace(':', "");
    if let Some(parent) = node.parent()
        && parent.is(NodeType::Pair)
        && parent.is_colon()
        && parent.child(0) == Some(node)
    {
        corrector.remove(parent.loc().operator.unwrap());
        boolean_literal = format!("{} =>", node.source());
    }
    corrector.replace(node.source_range(), boolean_literal);
}
