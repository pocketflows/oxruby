//! Port of `lib/rubocop/cop/style/if_unless_modifier_of_if_unless.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct IfUnlessModifierOfIfUnless;

impl Cop for IfUnlessModifierOfIfUnless {
    fn name(&self) -> &'static str {
        "Style/IfUnlessModifierOfIfUnless"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_modifier_form() || !node.body().is_some_and(|b| b.is(NodeType::If)) {
            return;
        }
        let message = format!(
            "Avoid modifier `{}` after another conditional.",
            node.keyword()
        );
        let if_branch = node.if_branch().unwrap();
        let condition = node.condition().unwrap();
        cx.add_offense_with_correction(node.loc().keyword.unwrap(), &message, |corrector| {
            corrector.wrap(
                if_branch.source_range(),
                format!("{} {}\n", node.keyword(), condition.source()),
                "\nend",
            );
            corrector.remove(
                if_branch
                    .source_range()
                    .end()
                    .join(condition.source_range().end()),
            );
        });
    }
}
