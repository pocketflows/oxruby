//! Port of `lib/rubocop/cop/lint/top_level_return_with_argument.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Top level return with argument detected.";

#[derive(Default)]
pub struct TopLevelReturnWithArgument;

impl Cop for TopLevelReturnWithArgument {
    fn name(&self) -> &'static str {
        "Lint/TopLevelReturnWithArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Return]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, return_node: Node<'a>) {
        if !top_level_return_with_any_argument(return_node) {
            return;
        }

        cx.add_offense_with_correction(return_node.source_range(), MSG, |corrector| {
            corrector.replace(return_node.source_range(), "return");
        });
    }
}

fn top_level_return_with_any_argument(return_node: Node<'_>) -> bool {
    top_level_return(return_node) && return_node.has_arguments()
}

fn top_level_return(return_node: Node<'_>) -> bool {
    !return_node
        .ancestors()
        .any(|a| a.is_any_block() || a.is_any_def())
}
