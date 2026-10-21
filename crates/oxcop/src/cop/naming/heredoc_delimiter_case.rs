//! Port of `lib/rubocop/cop/naming/heredoc_delimiter_case.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, delimiter_string, heredoc_node};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct HeredocDelimiterCase;

impl Cop for HeredocDelimiterCase {
    fn name(&self) -> &'static str {
        "Naming/HeredocDelimiterCase"
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
        let style = style(cx);
        let delimiter = delimiter_string(node.source());
        if delimiter == correct_delimiters(style, delimiter) {
            return;
        }
        let heredoc_end = node.loc().heredoc_end.unwrap();
        let message = format!("Use {style} heredoc delimiters.");
        cx.add_offense_with_correction(heredoc_end, &message, |corrector| {
            let expr = node.source_range();
            corrector.replace(expr, correct_delimiters(style, node.source()));
            corrector.replace(heredoc_end, correct_delimiters(style, delimiter));
        });
    }
}

fn correct_delimiters(style: &str, source: &str) -> String {
    if style == "uppercase" {
        source.to_uppercase()
    } else {
        source.to_lowercase()
    }
}
