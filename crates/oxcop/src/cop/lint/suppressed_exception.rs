//! Port of `lib/rubocop/cop/lint/suppressed_exception.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not suppress exceptions.";

#[derive(Default)]
pub struct SuppressedException;

impl Cop for SuppressedException {
    fn name(&self) -> &'static str {
        "Lint/SuppressedException"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.body().is_some() && !nil_body(node) {
            return;
        }
        if cx.cop_config_bool("AllowComments", false) && comment_between_rescue_and_end(cx, node) {
            return;
        }
        if cx.cop_config_bool("AllowNil", false) && nil_body(node) {
            return;
        }

        cx.add_offense(node.source_range(), MSG);
    }
}

fn comment_between_rescue_and_end(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(ancestor) = node
        .ancestors()
        .find(|a| a.is(NodeType::Kwbegin) || a.is_any_def() || a.is_any_block())
    else {
        return false;
    };
    let end_line = match ancestor.loc().end {
        Some(end) => ancestor.line_of(end),
        None => ancestor.last_line(),
    };
    let lines = cx.processed_source.lines();
    let first = node.first_line().min(lines.len());
    let last = end_line.min(lines.len());
    first < last && lines[first..last].iter().any(|line| comment_line(line))
}

fn nil_body(node: Node<'_>) -> bool {
    node.body().is_some_and(|b| b.is(NodeType::Nil))
}
