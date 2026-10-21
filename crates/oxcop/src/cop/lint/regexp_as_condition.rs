//! Port of `lib/rubocop/cop/lint/regexp_as_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const MSG: &str =
    "Do not use regexp literal as a condition. The regexp literal matches `$_` implicitly.";

#[derive(Default)]
pub struct RegexpAsCondition {
    ignored: IgnoredNodes,
}

impl Cop for RegexpAsCondition {
    fn name(&self) -> &'static str {
        "Lint/RegexpAsCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::MatchCurrentLine]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.ancestors().any(|a| a.is_conditional()) {
            return;
        }
        if self.ignored.part_of(node) {
            return;
        }

        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            match node.parent() {
                Some(parent) if parent.is(NodeType::Send) && parent.is_method("!") => {
                    corrector.replace(parent.source_range(), format!("!({} =~ $_)", node.source()));
                }
                _ => corrector.replace(node.source_range(), format!("{} =~ $_", node.source())),
            }
        });

        self.ignored.ignore(node);
    }
}
