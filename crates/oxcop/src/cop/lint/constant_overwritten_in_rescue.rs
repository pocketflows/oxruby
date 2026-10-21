//! Port of `lib/rubocop/cop/lint/constant_overwritten_in_rescue.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static OVERWRITTEN_CONSTANT: NodePattern = NodePattern::new("(resbody nil? $(casgn _ _) nil?)");

#[derive(Default)]
pub struct ConstantOverwrittenInRescue;

impl Cop for ConstantOverwrittenInRescue {
    fn name(&self) -> &'static str {
        "Lint/ConstantOverwrittenInRescue"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Naming/RescuedExceptionsVariableName", "Style/RescueStandardError"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(constant) = OVERWRITTEN_CONSTANT
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };
        let message = format!("`{}` is overwritten by `rescue =>`.", constant.source());
        let keyword = node.loc().keyword.unwrap();
        let assoc = node.loc().assoc.unwrap();
        cx.add_offense_with_correction(assoc, &message, |corrector| {
            corrector.remove(range_between(keyword.end_pos, assoc.end_pos));
        });
    }
}
