//! Port of `lib/rubocop/cop/style/if_with_boolean_literal_branches.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::util::indent;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_FOR_ELSIF: &str = "Use `else` instead of redundant `elsif` with boolean literal branches.";

static IF_WITH_BOOLEAN_LITERAL_BRANCHES: NodePattern =
    NodePattern::new("(if #return_boolean_value? <true false>)");
static DOUBLE_NEGATIVE: NodePattern = NodePattern::new("(send (send _ :!) :!)");

#[derive(Default)]
pub struct IfWithBooleanLiteralBranches;

impl Cop for IfWithBooleanLiteralBranches {
    fn name(&self) -> &'static str {
        "Style/IfWithBooleanLiteralBranches"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let matched = {
            let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
                name == "return_boolean_value?" && return_boolean_value(cx, val.node())
            };
            let ctx = MatchContext {
                call: Some(&call),
                ..Default::default()
            };
            IF_WITH_BOOLEAN_LITERAL_BRANCHES
                .match_with(node, &ctx)
                .is_some()
        };
        if !matched || multiple_elsif(node) {
            return;
        }
        let condition = node.condition().unwrap();
        let (range, keyword) = offense_range_with_keyword(node, condition);
        let message = if node.is_elsif() {
            MSG_FOR_ELSIF.to_owned()
        } else {
            format!("Remove redundant {keyword} with boolean literal branches.")
        };
        cx.add_offense_with_correction(range, &message, |corrector| {
            let replacement = replacement_condition(node, condition);
            if node.is_elsif() {
                corrector.insert_before(node.source_range(), "else\n");
                corrector.replace(
                    node.source_range(),
                    format!("{}{replacement}", indent(node.if_branch().unwrap(), 0)),
                );
            } else {
                corrector.replace(node.source_range(), replacement);
            }
        });
    }
}

fn multiple_elsif(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::If) && p.is_elsif())
}

fn offense_range_with_keyword(node: Node<'_>, condition: Node<'_>) -> (Range, String) {
    if node.is_ternary() {
        let range = condition
            .source_range()
            .end()
            .join(node.source_range().end());
        (range, "ternary operator".to_owned())
    } else {
        let keyword = node.loc().keyword.unwrap();
        (keyword, format!("`{}`", node.range_source(keyword)))
    }
}

fn return_boolean_value(cx: &Investigation<'_, '_>, condition: Option<Node<'_>>) -> bool {
    let Some(condition) = condition else {
        return false;
    };
    match condition.ty() {
        NodeType::Begin => return_boolean_value(cx, condition.child_nodes().next()),
        NodeType::Or => {
            return_boolean_value(cx, condition.lhs()) && return_boolean_value(cx, condition.rhs())
        }
        NodeType::And => return_boolean_value(cx, condition.rhs()),
        _ => assume_boolean_value(cx, condition),
    }
}

fn assume_boolean_value(cx: &Investigation<'_, '_>, condition: Node<'_>) -> bool {
    if !condition.is(NodeType::Send) {
        return false;
    }
    if condition
        .method_name()
        .is_some_and(|m| allowed_method(cx, m))
    {
        return false;
    }
    condition.is_comparison_method()
        || condition.is_predicate_method()
        || DOUBLE_NEGATIVE.is_match(condition)
}

fn replacement_condition(node: Node<'_>, condition: Node<'_>) -> String {
    if opposite_condition(node) {
        if require_parentheses(condition) {
            format!("!({})", condition.source())
        } else {
            format!("!{}", condition.source())
        }
    } else {
        condition.source().to_owned()
    }
}

fn opposite_condition(node: Node<'_>) -> bool {
    let if_branch = node.if_branch();
    (!node.is_unless() && if_branch.is_some_and(|b| b.is(NodeType::False)))
        || (node.is_unless() && if_branch.is_some_and(|b| b.is(NodeType::True)))
}

fn require_parentheses(condition: Node<'_>) -> bool {
    condition.is_operator_keyword()
        || (condition.is(NodeType::Send) && condition.is_comparison_method())
}
