//! Port of `lib/rubocop/cop/lint/empty_ensure.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Empty `ensure` block detected.";

#[derive(Default)]
pub struct EmptyEnsure;

impl Cop for EmptyEnsure {
    fn name(&self) -> &'static str {
        "Lint/EmptyEnsure"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Ensure]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.ensure_branch().is_some() {
            return;
        }
        let keyword = node.loc().keyword.unwrap();
        cx.add_offense_with_correction(keyword, MSG, |corrector| corrector.remove(keyword));
    }
}
