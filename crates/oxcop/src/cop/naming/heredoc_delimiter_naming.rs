//! Port of `lib/rubocop/cop/naming/heredoc_delimiter_naming.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, delimiter_string, heredoc_node};
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Use meaningful heredoc delimiters.";

#[derive(Default)]
pub struct HeredocDelimiterNaming;

impl Cop for HeredocDelimiterNaming {
    fn name(&self) -> &'static str {
        "Naming/HeredocDelimiterNaming"
    }

    fn node_types(&self) -> &'static [NodeType] {
        HEREDOC_NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = heredoc_node(node) else {
            return;
        };
        if meaningful_delimiters(cx, node) {
            return;
        }
        let range = if node.children().is_empty() {
            node.source_range()
        } else {
            node.loc().heredoc_end.unwrap()
        };
        cx.add_offense(range, MSG);
    }
}

fn meaningful_delimiters(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let delimiters = delimiter_string(node.source());
    if regex!(r"\w").find(delimiters).is_none() {
        return false;
    }
    cx.cop_config_list("ForbiddenDelimiters")
        .iter()
        .filter_map(|d| ruby_regex(d))
        .all(|re| re.find(delimiters).is_none())
}
