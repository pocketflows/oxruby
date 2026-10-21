//! Port of `lib/rubocop/cop/lint/redundant_require_statement.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove unnecessary `require` statement.";
const RUBY_22_LOADED_FEATURES: &[&str] = &["rational", "complex"];

static REDUNDANT_REQUIRE_STATEMENT: NodePattern =
    NodePattern::new("(send nil? :require (str $_))");

#[derive(Default)]
pub struct RedundantRequireStatement;

impl Cop for RedundantRequireStatement {
    fn name(&self) -> &'static str {
        "Lint/RedundantRequireStatement"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["require"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = REDUNDANT_REQUIRE_STATEMENT.match_node(node) else {
            return;
        };
        let Some(feature_name) = captures[0].str() else {
            return;
        };
        if !redundant_feature(cx, feature_name) {
            return;
        }

        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let range = match node.parent() {
                Some(parent) if parent.is_modifier_form() => {
                    corrector.insert_after(parent.source_range(), "\nend");
                    range_with_surrounding_space(
                        ps.buffer_source(),
                        node.source_range(),
                        SurroundingSpace::side(Side::Right),
                    )
                }
                _ => range_by_whole_lines(ps, node.source_range(), true),
            };
            corrector.remove(range);
        });
    }
}

fn redundant_feature(cx: &Investigation<'_, '_>, feature_name: &str) -> bool {
    let target_ruby_version = cx.target_ruby_version();
    feature_name == "enumerator"
        || (target_ruby_version >= 2.1 && feature_name == "thread")
        || (target_ruby_version >= 2.2 && RUBY_22_LOADED_FEATURES.contains(&feature_name))
        || (target_ruby_version >= 2.7 && feature_name == "ruby2_keywords")
        || (target_ruby_version >= 3.1 && feature_name == "fiber")
        || (target_ruby_version >= 3.2 && feature_name == "set")
        || (target_ruby_version >= 4.0 && feature_name == "pathname")
}
