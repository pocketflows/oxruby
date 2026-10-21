//! Port of `lib/rubocop/cop/style/super_with_args_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use parentheses for `super` with arguments.";

#[derive(Default)]
pub struct SuperWithArgsParentheses;

impl Cop for SuperWithArgsParentheses {
    fn name(&self) -> &'static str {
        "Style/SuperWithArgsParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Super]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_parenthesized() {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let (Some(first), Some(last)) = (node.first_argument(), node.last_argument()) else {
                return;
            };
            let range = node
                .loc()
                .keyword
                .unwrap()
                .end()
                .join(first.source_range().begin());
            corrector.replace(range, "(");
            corrector.insert_after(last.source_range(), ")");
        });
    }
}
