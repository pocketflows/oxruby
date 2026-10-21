//! Port of `lib/rubocop/cop/style/redundant_heredoc_delimiter_quotes.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, delimiter_string, heredoc_node, heredoc_type};
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct RedundantHeredocDelimiterQuotes;

impl Cop for RedundantHeredocDelimiterQuotes {
    fn name(&self) -> &'static str {
        "Style/RedundantHeredocDelimiterQuotes"
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
        if need_heredoc_delimiter_quotes(node) {
            return;
        }
        let replacement = format!(
            "{}{}",
            heredoc_type(node.source()),
            delimiter_string(node.source())
        );
        let message = format!(
            "Remove the redundant heredoc delimiter quotes, use `{replacement}` instead."
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), &replacement);
        });
    }
}

fn need_heredoc_delimiter_quotes(node: Node<'_>) -> bool {
    let ty = heredoc_type(node.source());
    let heredoc_delimiter: String = node.source().chars().filter(|c| !ty.contains(*c)).collect();
    if !heredoc_delimiter.starts_with(['\'', '"']) {
        return true;
    }
    let heredoc_end = node.range_source(node.loc().heredoc_end.unwrap());
    if regex!(r"\W").find(heredoc_end.trim()).is_some() {
        return true;
    }
    heredoc_delimiter.starts_with('\'')
        && regex!(r"#(\{|@|\$)|\\")
            .find(node.range_source(node.loc().heredoc_body.unwrap()))
            .is_some()
}
