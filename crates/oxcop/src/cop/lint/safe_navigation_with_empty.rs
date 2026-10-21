//! Port of `lib/rubocop/cop/lint/safe_navigation_with_empty.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid calling `empty?` with the safe navigation operator in conditionals.";

static SAFE_NAVIGATION_EMPTY_IN_CONDITIONAL: NodePattern =
    NodePattern::new("(if (csend !csend :empty?) ...)");

#[derive(Default)]
pub struct SafeNavigationWithEmpty;

impl Cop for SafeNavigationWithEmpty {
    fn name(&self) -> &'static str {
        "Lint/SafeNavigationWithEmpty"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !SAFE_NAVIGATION_EMPTY_IN_CONDITIONAL.is_match(node) {
            return;
        }

        let Some(condition) = node.condition() else {
            return;
        };
        cx.add_offense_with_correction(condition.source_range(), MSG, |corrector| {
            let receiver = condition.receiver().map(|r| r.source()).unwrap_or_default();
            corrector.replace(
                condition.source_range(),
                format!(
                    "{receiver} && {receiver}.{}",
                    condition.method_name().unwrap_or_default()
                ),
            );
        });
    }
}
