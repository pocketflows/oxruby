//! Port of `lib/rubocop/cop/lint/duplicate_match_pattern.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Duplicate `in` pattern detected.";

#[derive(Default)]
pub struct DuplicateMatchPattern;

impl Cop for DuplicateMatchPattern {
    fn name(&self) -> &'static str {
        "Lint/DuplicateMatchPattern"
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::CaseMatch]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
        let mut previous: Vec<String> = Vec::new();
        for in_pattern_node in case_node.when_branches() {
            let Some(pattern) = in_pattern_node.child(0) else {
                continue;
            };
            let identity = pattern_identity(pattern);
            if previous.contains(&identity) {
                cx.add_offense(pattern.source_range(), MSG);
            } else {
                previous.push(identity);
            }
        }
    }
}

fn pattern_identity(pattern: Node<'_>) -> String {
    let pattern_source = if pattern.is_any(&[NodeType::HashPattern, NodeType::MatchAlt]) {
        let mut sources: Vec<&str> = pattern.child_nodes().map(|c| c.source()).collect();
        sources.sort_unstable();
        ruby_array_inspect(&sources)
    } else {
        pattern.source().to_owned()
    };
    match pattern.parent().and_then(|p| p.child(1)) {
        Some(guard) => pattern_source + guard.source(),
        None => pattern_source,
    }
}

/// `Array#to_s` of strings.
fn ruby_array_inspect(items: &[&str]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|s| crate::cop::mixin::util::ruby_inspect(s))
        .collect();
    format!("[{}]", inner.join(", "))
}
