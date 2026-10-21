//! Port of `lib/rubocop/cop/style/redundant_interpolation_unfreeze.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::frozen_string_literal::{uninterpolated_heredoc, uninterpolated_string};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Don't unfreeze interpolated strings as they are already unfrozen.";

static REDUNDANT_UNFREEZE: NodePattern = NodePattern::new(
    "{
      (send dstr_type? {:+@ :dup})
      (send (const nil? :String) :new dstr_type?)
    }",
);

#[derive(Default)]
pub struct RedundantInterpolationUnfreeze;

impl Cop for RedundantInterpolationUnfreeze {
    fn name(&self) -> &'static str {
        "Style/RedundantInterpolationUnfreeze"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if uninterpolated_string(node) || uninterpolated_heredoc(node) {
            return;
        }
        let Some(parent) = node.parent().filter(|p| REDUNDANT_UNFREEZE.is_match(*p)) else {
            return;
        };
        let range = if parent.is_method("new") {
            parent
                .source_range()
                .begin()
                .join(parent.loc().selector.unwrap())
        } else {
            parent.loc().selector.unwrap()
        };
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(parent.source_range(), node.source());
        });
    }
}
