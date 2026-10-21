//! Port of `lib/rubocop/cop/lint/empty_interpolation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::interpolation::{NODE_TYPES, interpolations};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Empty interpolation detected.";

#[derive(Default)]
pub struct EmptyInterpolation;

impl Cop for EmptyInterpolation {
    fn name(&self) -> &'static str {
        "Lint/EmptyInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for begin_node in interpolations(node) {
            on_interpolation(cx, begin_node);
        }
    }
}

fn on_interpolation<'a>(cx: &mut Investigation<'a, '_>, begin_node: Node<'a>) {
    if in_percent_literal_array(begin_node) {
        return;
    }
    let all_empty = begin_node.child_nodes().all(|e| {
        e.is(NodeType::Nil) || (e.is_basic_literal() && e.str_content().is_some_and(str::is_empty))
    });
    if !all_empty {
        return;
    }
    cx.add_offense_with_correction(begin_node.source_range(), MSG, |corrector| {
        corrector.remove(begin_node.source_range());
    });
}

fn in_percent_literal_array(begin_node: Node<'_>) -> bool {
    begin_node
        .ancestors()
        .find(|a| a.is(NodeType::Array))
        .is_some_and(|array| array.is_percent_literal())
}
