//! Port of `lib/rubocop/cop/style/min_max_comparison.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const GREATER_OPERATORS: &[&str] = &[">", ">="];
const LESS_OPERATORS: &[&str] = &["<", "<="];
const COMPARISON_OPERATORS: &[&str] = &[">", ">=", "<", "<="];

static COMPARISON_CONDITION: NodePattern = NodePattern::new(
    "{
      (send $_lhs $COMPARISON_OPERATORS $_rhs)
      (begin (send $_lhs $COMPARISON_OPERATORS $_rhs))
    }",
);

#[derive(Default)]
pub struct MinMaxComparison;

impl Cop for MinMaxComparison {
    fn name(&self) -> &'static str {
        "Style/MinMaxComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(condition) = node.condition() else {
            return;
        };
        let ctx = MatchContext {
            named: &[("COMPARISON_OPERATORS", Param::Syms(COMPARISON_OPERATORS))],
            ..Default::default()
        };
        let Some(captures) = COMPARISON_CONDITION.match_with(condition, &ctx) else {
            return;
        };
        let (Some(lhs), Some(operator), Some(rhs)) =
            (captures[0].node(), captures[1].sym(), captures[2].node())
        else {
            return;
        };
        // For `unless`, the branches run opposite to an `if`, so swap them to
        // keep the `max`/`min` decision correct.
        let mut if_branch = node.if_branch();
        let mut else_branch = node.else_branch();
        if node.is_unless() {
            std::mem::swap(&mut if_branch, &mut else_branch);
        }
        let Some(preferred_method) = preferred_method(operator, lhs, rhs, if_branch, else_branch)
        else {
            return;
        };
        let replacement = format!("[{}, {}].{preferred_method}", lhs.source(), rhs.source());
        let message = format!("Use `{replacement}` instead.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if node.is_elsif() {
                corrector.remove(range_between(
                    node.parent().unwrap().loc().else_.unwrap().begin_pos,
                    node.loc().else_.unwrap().begin_pos,
                ));
                corrector.replace(node.else_branch().unwrap().source_range(), &replacement);
            } else {
                corrector.replace(node.source_range(), &replacement);
            }
        });
    }
}

fn same(a: Node<'_>, b: Option<Node<'_>>) -> bool {
    b.is_some_and(|b| a.same_as(b))
}

fn preferred_method(
    operator: &str,
    lhs: Node<'_>,
    rhs: Node<'_>,
    if_branch: Option<Node<'_>>,
    else_branch: Option<Node<'_>>,
) -> Option<&'static str> {
    if same(lhs, if_branch) && same(rhs, else_branch) {
        Some(if GREATER_OPERATORS.contains(&operator) { "max" } else { "min" })
    } else if same(lhs, else_branch) && same(rhs, if_branch) {
        Some(if LESS_OPERATORS.contains(&operator) { "max" } else { "min" })
    } else {
        None
    }
}
