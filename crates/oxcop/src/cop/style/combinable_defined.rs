//! Port of `lib/rubocop/cop/style/combinable_defined.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Combine nested `defined?` calls.";
const OPERATORS: &[&str] = &["&&", "and"];

#[derive(Default)]
pub struct CombinableDefined;

impl Cop for CombinableDefined {
    fn name(&self) -> &'static str {
        "Style/CombinableDefined"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::And]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let terms = terms(node);
        if !terms.iter().all(|t| t.is(NodeType::Defined)) {
            return;
        }
        let calls = defined_calls(&terms);
        let namespaces = namespaces(&calls);
        let src = cx.processed_source.buffer_source();
        for call in calls {
            if !namespaces.iter().any(|ns| ns.same_as(call)) {
                continue;
            }
            cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                remove_term(src, corrector, call);
            });
        }
    }
}

fn terms(node: Node<'_>) -> Vec<Node<'_>> {
    node.descendants()
        .filter(|d| d.parent().is_some_and(|p| p.is(NodeType::And)) && !d.is(NodeType::And))
        .collect()
}

fn defined_calls<'a>(nodes: &[Node<'a>]) -> Vec<Node<'a>> {
    nodes
        .iter()
        .filter_map(|defined_node| defined_node.first_argument())
        .filter(|subject| subject.is(NodeType::Const) || subject.is_call())
        .collect()
}

fn namespaces<'a>(nodes: &[Node<'a>]) -> Vec<Node<'a>> {
    nodes
        .iter()
        .filter_map(|node| {
            if node.is(NodeType::Const) {
                node.namespace()
            } else {
                node.receiver()
            }
        })
        .collect()
}

fn remove_term(src: &str, corrector: &mut Corrector, term: Node<'_>) {
    let mut term = term;
    while let Some(parent) = term.parent().filter(|p| !p.is(NodeType::And)) {
        term = parent;
    }
    let parent = term.parent().unwrap();
    let range = if parent.child_from_end(1) == Some(term) {
        rhs_range_to_remove(src, term)
    } else {
        lhs_range_to_remove(src, term)
    };
    corrector.remove(range);
}

fn lhs_range_to_remove(src: &str, term: Node<'_>) -> Range {
    let bytes = src.as_bytes();
    let mut pos = term.source_range().end_pos;
    while !OPERATORS
        .iter()
        .any(|op| bytes[..=pos.min(bytes.len() - 1)].ends_with(op.as_bytes()))
    {
        pos += 1;
    }
    range_with_surrounding_space(
        src,
        term.source_range().with_end(pos + 1),
        SurroundingSpace {
            side: Side::Right,
            newlines: false,
            ..SurroundingSpace::default()
        },
    )
}

fn rhs_range_to_remove(src: &str, term: Node<'_>) -> Range {
    let bytes = src.as_bytes();
    let mut pos = term.source_range().begin_pos;
    while !OPERATORS
        .iter()
        .any(|op| bytes[pos..(pos + 3).min(bytes.len())].starts_with(op.as_bytes()))
    {
        pos -= 1;
    }
    range_with_surrounding_space(
        src,
        term.source_range().with_begin(pos - 1),
        SurroundingSpace {
            side: Side::Right,
            newlines: false,
            ..SurroundingSpace::default()
        },
    )
}
