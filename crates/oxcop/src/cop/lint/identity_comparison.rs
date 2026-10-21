//! Port of `lib/rubocop/cop/lint/identity_comparison.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static OBJECT_ID_COMPARISON: NodePattern = NodePattern::new(
    "(send
      (send
        _lhs_receiver :object_id) ${:== :!=}
      (send
        _rhs_receiver :object_id))",
);

#[derive(Default)]
pub struct IdentityComparison;

impl Cop for IdentityComparison {
    fn name(&self) -> &'static str {
        "Lint/IdentityComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "!="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = OBJECT_ID_COMPARISON.match_node(node) else {
            return;
        };
        let comparison_method = captures[0].sym().unwrap();
        let bang = if comparison_method == "==" { "" } else { "!" };
        let message = format!(
            "Use `{bang}equal?` instead of `{comparison_method}` when comparing `object_id`."
        );
        // The Ruby block `return`s from `on_send` before the offense is
        // recorded when either side has no receiver.
        let receiver = node.receiver().and_then(|r| r.receiver());
        let argument = node.first_argument().and_then(|a| a.receiver());
        let (Some(receiver), Some(argument)) = (receiver, argument) else {
            return;
        };
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let replacement = format!(
                "{bang}{}.equal?({})",
                receiver.source(),
                argument.source()
            );
            corrector.replace(node.source_range(), replacement);
        });
    }
}
