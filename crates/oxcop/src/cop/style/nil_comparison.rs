//! Port of `lib/rubocop/cop/style/nil_comparison.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

const PREDICATE_MSG: &str = "Prefer the use of the `nil?` predicate.";
const EXPLICIT_MSG: &str = "Prefer the use of the `==` comparison.";

static NIL_COMPARISON: NodePattern = NodePattern::new("(send _ {:== :===} nil)");
static NIL_CHECK: NodePattern = NodePattern::new("(send _ :nil?)");

#[derive(Default)]
pub struct NilComparison;

impl Cop for NilComparison {
    fn name(&self) -> &'static str {
        "Style/NilComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "===", "nil?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_none() {
            return;
        }
        let prefer_comparison = style(cx) == "comparison";
        let matched = if prefer_comparison {
            NIL_CHECK.is_match(node)
        } else {
            NIL_COMPARISON.is_match(node)
        };
        if !matched {
            return;
        }
        let message = if prefer_comparison {
            EXPLICIT_MSG
        } else {
            PREDICATE_MSG
        };
        cx.add_offense_with_correction(node.loc().selector.unwrap(), message, |corrector| {
            if prefer_comparison {
                autocorrect_to_comparison(corrector, node);
            } else {
                autocorrect_to_predicate(corrector, node);
            }
        });
    }
}

fn autocorrect_to_comparison(corrector: &mut Corrector, node: Node<'_>) {
    let range = node
        .loc()
        .dot
        .unwrap()
        .join(node.loc().selector.unwrap().end());
    corrector.replace(range, " == nil");
    if operator_expression(node.parent()) {
        corrector.wrap(node.source_range(), "(", ")");
    }
}

fn autocorrect_to_predicate(corrector: &mut Corrector, node: Node<'_>) {
    let receiver = node.receiver().unwrap();
    if operator_expression(Some(receiver)) {
        corrector.replace(node.source_range(), format!("({}).nil?", receiver.source()));
    } else {
        let range = receiver.source_range().end().join(node.source_range().end());
        corrector.replace(range, ".nil?");
    }
}

pub(crate) fn operator_expression(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    operator_send(node)
        || node.is_operator_keyword()
        || (node.is(NodeType::If) && node.is_ternary())
        || matches!(
            node.ty(),
            NodeType::Irange | NodeType::Erange | NodeType::Iflipflop | NodeType::Eflipflop
        )
        || node.is_assignment()
}

fn operator_send(node: Node<'_>) -> bool {
    node.is_send() && node.is_operator_method() && !node.is_method("[]") && !node.is_method("[]=")
}
