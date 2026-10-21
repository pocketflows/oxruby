//! Port of `lib/rubocop/cop/lint/nested_percent_literal.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::percent_literal::process;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Within percent literals, nested percent literals do not function and may be unwanted in the result.";

/// `PreferredDelimiters::PERCENT_LITERAL_TYPES`.
const PERCENT_LITERAL_TYPES: &[&str] = &["%", "%i", "%I", "%q", "%Q", "%r", "%s", "%w", "%W", "%x"];

#[derive(Default)]
pub struct NestedPercentLiteral;

impl Cop for NestedPercentLiteral {
    fn name(&self) -> &'static str {
        "Lint/NestedPercentLiteral"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, PERCENT_LITERAL_TYPES) {
            return;
        }
        if contains_percent_literals(node) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}

fn contains_percent_literals(node: Node<'_>) -> bool {
    node.child_nodes().any(|child| {
        // `child.children.first.to_s`: a node child (`dstr`) prints as an
        // s-expression, which starts with `(` and never matches.
        let literal = match child.children().first() {
            Some(Child::Sym(s) | Child::Str(s)) => s.clone(),
            Some(Child::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => return false,
        };
        PERCENT_LITERAL_TYPES.iter().any(|percent_literal| {
            literal
                .strip_prefix(percent_literal)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|c| !(c.is_ascii_alphanumeric() || c == '_'))
        })
    })
}
