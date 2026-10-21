//! Port of `lib/rubocop/cop/lint/numeric_operation_with_constant_result.rb`.

use crate::ast::node_pattern::{Capture, NodePattern};
use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Numeric operation with a constant result detected.";

static OPERATION_WITH_CONSTANT_RESULT: NodePattern =
    NodePattern::new("(call (call nil? $_lhs) $_operation ({int | call nil?} $_rhs))");
static ABBREVIATED_ASSIGNMENT_WITH_CONSTANT_RESULT: NodePattern =
    NodePattern::new("(op-asgn (lvasgn $_lhs) $_operation ({int lvar} $_rhs))");

#[derive(Default)]
pub struct NumericOperationWithConstantResult;

impl Cop for NumericOperationWithConstantResult {
    fn name(&self) -> &'static str {
        "Lint/NumericOperationWithConstantResult"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::OpAsgn]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["*", "/", "**"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Csend) {
            return;
        }
        if node.is(NodeType::OpAsgn) {
            let Some(captures) = ABBREVIATED_ASSIGNMENT_WITH_CONSTANT_RESULT.match_node(node) else {
                return;
            };
            let Some((lhs, result)) = constant_result_of(&captures) else {
                return;
            };
            cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                corrector.replace(node.source_range(), format!("{lhs} = {result}"));
            });
            return;
        }
        let Some(captures) = OPERATION_WITH_CONSTANT_RESULT.match_node(node) else {
            return;
        };
        let Some((_, result)) = constant_result_of(&captures) else {
            return;
        };
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), result.to_string());
        });
    }
}

/// The value's `to_s`.
fn value_to_s(capture: &Capture<'_>) -> Option<String> {
    match capture.val()? {
        crate::ast::node_pattern::Val::Value(Child::Sym(s)) => Some(s.clone()),
        crate::ast::node_pattern::Val::Value(Child::Int(i)) => Some(i.to_string()),
        crate::ast::node_pattern::Val::Value(Child::BigInt(s)) => Some(s.clone()),
        _ => None,
    }
}

fn constant_result_of(captures: &[Capture<'_>]) -> Option<(String, i64)> {
    let lhs = captures[0].sym()?;
    let operation = captures[1].sym()?;
    let rhs = &captures[2];
    let result = constant_result(lhs, operation, rhs)?;
    Some((lhs.to_owned(), result))
}

fn constant_result(lhs: &str, operation: &str, rhs: &Capture<'_>) -> Option<i64> {
    if value_to_s(rhs).as_deref() == Some("0") {
        return match operation {
            "*" => Some(0),
            "**" => Some(1),
            _ => None,
        };
    }
    // `rhs == lhs`: a symbol equals the symbol (an integer never does).
    if rhs.sym() == Some(lhs) && operation == "/" {
        return Some(1);
    }
    None
}
