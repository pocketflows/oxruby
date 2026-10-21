//! Port of `lib/rubocop/cop/gemspec/duplicated_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::gemspec_help::{
    assignment_method_declarations, gem_specification_p, indexed_assignment_method_declarations,
};
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct DuplicatedAssignment;

impl Cop for DuplicatedAssignment {
    fn name(&self) -> &'static str {
        "Gemspec/DuplicatedAssignment"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(ast) = ps.ast() else {
            return;
        };
        // `process_assignment_method_nodes`
        let mut groups: Vec<(AssignmentKey, Vec<Node<'_>>)> = Vec::new();
        for node in assignment_method_declarations(ps, ast) {
            if !node.is_assignment_method() {
                continue;
            }
            let key = (
                enclosing_specification(node).map(|n| n.id()),
                node.method_name().unwrap_or("").to_owned(),
            );
            push_group(&mut groups, key, node);
        }
        for (_, nodes) in groups.iter().filter(|(_, nodes)| nodes.len() > 1) {
            for node in &nodes[1..] {
                register_offense(cx, *node, node.method_name().unwrap_or(""), nodes[0].first_line());
            }
        }

        // `process_indexed_assignment_method_nodes`
        let mut indexed: Vec<(IndexedKey<'_>, Vec<Node<'_>>)> = Vec::new();
        for node in indexed_assignment_method_declarations(ps, ast) {
            let key = IndexedKey {
                specification: enclosing_specification(node).map(|n| n.id()),
                attribute: node.child(0).and_then(|c| c.method_name()).unwrap_or("").to_owned(),
                argument: node.first_argument(),
            };
            match indexed.iter_mut().find(|(k, _)| k.same_as(&key)) {
                Some((_, nodes)) => nodes.push(node),
                None => indexed.push((key, vec![node])),
            }
        }
        for (_, nodes) in indexed.iter().filter(|(_, nodes)| nodes.len() > 1) {
            for node in &nodes[1..] {
                let assignment = format!(
                    "{}[{}]=",
                    node.child(0).and_then(|c| c.method_name()).unwrap_or(""),
                    node.first_argument().map_or("", |a| a.source())
                );
                register_offense(cx, *node, &assignment, nodes[0].first_line());
            }
        }
    }
}

/// `[enclosing_specification(node), node.method_name]`.
type AssignmentKey = (Option<u32>, String);

struct IndexedKey<'a> {
    specification: Option<u32>,
    attribute: String,
    argument: Option<Node<'a>>,
}

impl IndexedKey<'_> {
    fn same_as(&self, other: &IndexedKey<'_>) -> bool {
        self.specification == other.specification
            && self.attribute == other.attribute
            && match (self.argument, other.argument) {
                (Some(a), Some(b)) => a.same_as(b),
                (None, None) => true,
                _ => false,
            }
    }
}

fn push_group<'a, K: PartialEq>(groups: &mut Vec<(K, Vec<Node<'a>>)>, key: K, node: Node<'a>) {
    match groups.iter_mut().find(|(k, _)| *k == key) {
        Some((_, nodes)) => nodes.push(node),
        None => groups.push((key, vec![node])),
    }
}

/// Assignments in separate `Gem::Specification.new` blocks are not
/// duplicates of one another, so the enclosing specification is part of the
/// grouping key.
fn enclosing_specification(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors()
        .filter(|a| a.is(NodeType::Block))
        .find(|block| gem_specification_p(*block).is_some())
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    assignment: &str,
    line_of_first_occurrence: usize,
) {
    let length = node.last_column().saturating_sub(node.column());
    let offense_location = source_range(cx.processed_source, node.first_line(), node.column(), length);
    let message = format!(
        "`{assignment}` method calls already given on line {line_of_first_occurrence} of the gemspec."
    );
    cx.add_offense(offense_location, &message);
}
