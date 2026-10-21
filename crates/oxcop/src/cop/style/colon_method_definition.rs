//! Port of `lib/rubocop/cop/style/colon_method_definition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use `::` for defining class methods.";

#[derive(Default)]
pub struct ColonMethodDefinition;

impl Cop for ColonMethodDefinition {
    fn name(&self) -> &'static str {
        "Style/ColonMethodDefinition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(operator) = node.loc().operator else {
            return;
        };
        if node.range_source(operator) != "::" {
            return;
        }
        cx.add_offense_with_correction(operator, MSG, |corrector| {
            corrector.replace(operator, ".");
        });
    }
}
