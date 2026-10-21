//! Port of `lib/rubocop/cop/style/ambiguous_endless_method_definition.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::endless_method_rewriter::{
    configured_indentation_width, correct_to_multiline,
};
use crate::cop::{Cop, Investigation};

static AMBIGUOUS_ENDLESS_METHOD_BODY: NodePattern = NodePattern::new(
    "^${
       (if _ <def _>)
       ({and or} def _)
       ({while until} _ def)
     }",
);

#[derive(Default)]
pub struct AmbiguousEndlessMethodDefinition;

impl Cop for AmbiguousEndlessMethodDefinition {
    fn name(&self) -> &'static str {
        "Style/AmbiguousEndlessMethodDefinition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_endless() {
            return;
        }
        let Some(operation) = AMBIGUOUS_ENDLESS_METHOD_BODY
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };
        if !(operation.is_operator_keyword() || operation.is_modifier_form()) {
            return;
        }
        let keyword = if operation.is_operator_keyword() {
            operation.operator().unwrap_or("")
        } else {
            operation.keyword()
        };
        let message = format!("Avoid using `{keyword}` statements with endless methods.");
        let width = configured_indentation_width(cx);
        cx.add_offense_with_correction(operation.source_range(), &message, |corrector| {
            correct_to_multiline(corrector, node, width);
        });
    }
}
