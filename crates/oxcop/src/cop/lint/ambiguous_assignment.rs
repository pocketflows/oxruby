//! Port of `lib/rubocop/cop/lint/ambiguous_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::check_assignment;
use crate::cop::mixin::range_help::{char_offset, range_between};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Suspicious assignment detected. Did you mean `%<op>s`?";

const MISTAKES: &[(&str, &str)] = &[("=-", "-="), ("=+", "+="), ("=*", "*="), ("=!", "!=")];

#[derive(Default)]
pub struct AmbiguousAssignment;

const NODE_TYPES: &[NodeType] = &[
    NodeType::Lvasgn,
    NodeType::Ivasgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
    NodeType::Casgn,
    NodeType::Masgn,
    NodeType::OpAsgn,
    NodeType::OrAsgn,
    NodeType::AndAsgn,
    NodeType::Send,
    NodeType::Csend,
];

impl Cop for AmbiguousAssignment {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousAssignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // `alias on_csend on_send`
        let call = if node.is(NodeType::Csend) {
            check_assignment::extract_rhs(node).map(|rhs| (node, Some(rhs)))
        } else {
            check_assignment::assignment(node)
        };
        if let Some((node, rhs)) = call {
            check(cx, node, rhs);
        }
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, rhs: Option<Node<'a>>) {
    let Some(rhs) = rhs else {
        return;
    };
    let Some(operator) = node.loc().operator else {
        return;
    };
    let source = cx.processed_source.buffer_source();
    let begin = operator.end_pos - 1;
    let rhs_begin = rhs.source_range().begin_pos;
    let end = char_offset(source, rhs_begin, 1);
    if end < begin {
        return;
    }
    let range = range_between(begin, end);
    let text = cx.source(range);
    let Some((_, op)) = MISTAKES.iter().find(|(mistake, _)| *mistake == text) else {
        return;
    };
    cx.add_offense(range, &MSG.replace("%<op>s", op));
}
