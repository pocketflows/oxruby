//! Port of `lib/rubocop/cop/gemspec/attribute_assignment.rb`.

use crate::ast::Node;
use crate::cop::mixin::gemspec_help::{
    assignment_method_declarations, indexed_assignment_method_declarations,
};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use consistent style for Gemspec attributes assignment.";

#[derive(Default)]
pub struct AttributeAssignment;

impl Cop for AttributeAssignment {
    fn name(&self) -> &'static str {
        "Gemspec/AttributeAssignment"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(ast) = ps.ast() else {
            return;
        };
        // `source_assignments`: attribute names without the `=`.
        let mut assignments: Vec<String> = Vec::new();
        for node in assignment_method_declarations(ps, ast) {
            if !node.is_assignment_method() {
                continue;
            }
            let name = node.method_name().unwrap_or("");
            let name = name.strip_suffix('=').unwrap_or(name).to_owned();
            if !assignments.contains(&name) {
                assignments.push(name);
            }
        }
        // `source_indexed_assignments`, grouped by the attribute.
        let mut indexed: Vec<(String, Vec<Node<'_>>)> = Vec::new();
        for node in indexed_assignment_method_declarations(ps, ast) {
            let name = node
                .child(0)
                .and_then(|c| c.method_name())
                .unwrap_or("")
                .to_owned();
            match indexed.iter_mut().find(|(k, _)| *k == name) {
                Some((_, nodes)) => nodes.push(node),
                None => indexed.push((name, vec![node])),
            }
        }
        for attribute in &assignments {
            if let Some((_, nodes)) = indexed.iter().find(|(k, _)| k == attribute) {
                for node in nodes {
                    cx.add_offense(node.source_range(), MSG);
                }
            }
        }
    }
}
