//! Port of `lib/rubocop/cop/style/unpack_first.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static UNPACK_AND_FIRST_ELEMENT: NodePattern = NodePattern::new(
    "{
       (call $(call (...) :unpack $(...)) :first)
       (call $(call (...) :unpack $(...)) {:[] :slice :at} (int 0))
     }",
);

#[derive(Default)]
pub struct UnpackFirst;

impl Cop for UnpackFirst {
    fn name(&self) -> &'static str {
        "Style/UnpackFirst"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.4)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["first", "[]", "slice", "at"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = UNPACK_AND_FIRST_ELEMENT.match_node(node) else {
            return;
        };
        let (Some(unpack_call), Some(unpack_arg)) = (captures[0].node(), captures[1].node()) else {
            return;
        };
        let first_element_range = unpack_call
            .source_range()
            .end()
            .join(node.source_range().end());
        let selector = unpack_call.loc().selector.unwrap();
        let offense_range = selector.join(node.source_range().end());
        let message = format!(
            "Use `unpack1({})` instead of `{}`.",
            unpack_arg.source(),
            cx.source(offense_range)
        );
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            corrector.remove(first_element_range);
            corrector.replace(selector, "unpack1");
        });
    }
}
