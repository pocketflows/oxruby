//! Port of `lib/rubocop/cop/style/in_pattern_then.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct InPatternThen;

impl Cop for InPatternThen {
    fn name(&self) -> &'static str {
        "Style/InPatternThen"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::InPattern]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_multiline() || node.has_then() || node.body().is_none() {
            return;
        }
        let Some(pattern) = node.child(0) else {
            return;
        };
        let pattern_source = if pattern.is(NodeType::MatchAlt) {
            collect_alternative_patterns(pattern).join(" | ")
        } else {
            pattern.source().to_owned()
        };
        let begin = node.loc().begin.unwrap();
        let message =
            format!("Do not use `in {pattern_source};`. Use `in {pattern_source} then` instead.");
        cx.add_offense_with_correction(begin, &message, |corrector| {
            corrector.replace(begin, " then");
        });
    }
}

fn collect_alternative_patterns(pattern: Node<'_>) -> Vec<&str> {
    let first = pattern.child(0).unwrap();
    if !first.is(NodeType::MatchAlt) {
        return pattern.child_nodes().map(|c| c.source()).collect();
    }
    let mut pattern_sources = collect_alternative_patterns(first);
    pattern_sources.push(pattern.child(1).unwrap().source());
    pattern_sources
}
