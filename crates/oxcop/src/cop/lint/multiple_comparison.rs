//! Port of `lib/rubocop/cop/lint/multiple_comparison.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use the `&&` operator to compare multiple values.";
const SET_OPERATION_OPERATORS: &[&str] = &["&", "|", "^"];

static MULTIPLE_COMPARE: NodePattern =
    NodePattern::new("(send (send _ {:< :> :<= :>=} $_) {:< :> :<= :>=} _)");

#[derive(Default)]
pub struct MultipleComparison;

impl Cop for MultipleComparison {
    fn name(&self) -> &'static str {
        "Lint/MultipleComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["<", ">", "<=", ">="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(center) = MULTIPLE_COMPARE
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };
        if center.is_send()
            && center
                .method_name()
                .is_some_and(|m| SET_OPERATION_OPERATORS.contains(&m))
        {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let new_center = format!("{} && {}", center.source(), center.source());
            corrector.replace(center.source_range(), new_center);
        });
    }
}
