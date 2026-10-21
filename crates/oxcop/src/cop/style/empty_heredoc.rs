//! Port of `lib/rubocop/cop/style/empty_heredoc.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, heredoc_node};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::mixin::string_literals_help::preferred_string_literal;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use an empty string literal instead of heredoc.";

#[derive(Default)]
pub struct EmptyHeredoc;

impl Cop for EmptyHeredoc {
    fn name(&self) -> &'static str {
        "Style/EmptyHeredoc"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        HEREDOC_NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = heredoc_node(node) else {
            return;
        };
        if node.is(NodeType::Xstr) {
            return;
        }
        let heredoc_body = node.loc().heredoc_body.unwrap();
        if !heredoc_body.is_empty() {
            return;
        }
        let ps = cx.processed_source;
        let preferred = preferred_string_literal(cx);
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let heredoc_end = node.loc().heredoc_end.unwrap();
            corrector.replace(node.source_range(), preferred);
            corrector.remove(range_by_whole_lines(ps, heredoc_body, true));
            corrector.remove(range_by_whole_lines(ps, heredoc_end, true));
        });
    }
}
