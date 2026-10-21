//! Port of `lib/rubocop/cop/lint/triple_quotes.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "Delimiting a string with multiple quotes has no effect, use a single quote instead.";

#[derive(Default)]
pub struct TripleQuotes;

impl Cop for TripleQuotes {
    fn name(&self) -> &'static str {
        "Lint/TripleQuotes"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let mut empty_str_nodes = empty_str_nodes(node);
        if empty_str_nodes.is_empty() {
            return;
        }

        let opening_quotes = node
            .source()
            .chars()
            .take_while(|&c| c == '\'' || c == '"')
            .count();
        if opening_quotes < 3 {
            return;
        }

        if empty_str_nodes.len() == node.child_nodes().count() {
            empty_str_nodes.remove(0);
        }

        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            for str in &empty_str_nodes {
                corrector.remove(str.source_range());
            }
        });
    }
}

fn empty_str_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    node.child_nodes()
        .filter(|c| c.is(NodeType::Str) && c.str_content() == Some(""))
        .collect()
}
