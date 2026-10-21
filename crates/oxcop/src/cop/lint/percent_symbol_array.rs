//! Port of `lib/rubocop/cop/lint/percent_symbol_array.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::percent_literal::process;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Within `%i`/`%I`, ':' and ',' are unnecessary and may be unwanted in the resulting symbols.";

#[derive(Default)]
pub struct PercentSymbolArray;

impl Cop for PercentSymbolArray {
    fn name(&self) -> &'static str {
        "Lint/PercentSymbolArray"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, &["%i", "%I"]) {
            return;
        }
        if !contains_colons_or_commas(node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            autocorrect(corrector, node);
        });
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    for child in node.child_nodes() {
        let range = child.source_range();
        if child.source().ends_with(',') {
            corrector.remove_trailing(range, 1);
        }
        if child.source().starts_with(':') {
            corrector.remove_leading(range, 1);
        }
    }
}

fn contains_colons_or_commas(node: Node<'_>) -> bool {
    node.child_nodes().any(|child| {
        // A `dsym`'s first child prints as an s-expression: `(str ...)`.
        let literal = match child.children().first() {
            Some(Child::Sym(s) | Child::Str(s)) => s.clone(),
            Some(Child::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => return false,
        };
        if non_alphanumeric_literal(&literal) {
            return false;
        }
        literal.starts_with(':') || literal.ends_with(',')
    })
}

fn non_alphanumeric_literal(literal: &str) -> bool {
    !literal.chars().any(char::is_alphanumeric)
}
