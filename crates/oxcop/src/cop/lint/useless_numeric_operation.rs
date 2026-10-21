//! Port of `lib/rubocop/cop/lint/useless_numeric_operation.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not apply inconsequential numeric operations to variables.";

static USELESS_OPERATION: NodePattern =
    NodePattern::new("(call ${lvar ivar cvar gvar const (send nil? _)} $_ (int $_))");
static USELESS_ABBREVIATED_ASSIGNMENT: NodePattern =
    NodePattern::new("(op-asgn ${lvasgn ivasgn cvasgn gvasgn casgn} $_ (int $_))");

#[derive(Default)]
pub struct UselessNumericOperation;

impl Cop for UselessNumericOperation {
    fn name(&self) -> &'static str {
        "Lint/UselessNumericOperation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::OpAsgn]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["+", "-", "*", "/", "**"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::OpAsgn) {
            on_op_asgn(cx, node);
            return;
        }
        let Some(captures) = USELESS_OPERATION.match_node(node) else {
            return;
        };
        let (Some(receiver), Some(operation), Some(number)) = (
            captures[0].node(),
            captures[1].sym(),
            captures[2].val().and_then(|v| v.int()),
        ) else {
            return;
        };
        if !useless(operation, number) {
            return;
        }

        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), receiver.source());
        });
    }
}

fn on_op_asgn<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(captures) = USELESS_ABBREVIATED_ASSIGNMENT.match_node(node) else {
        return;
    };
    let (Some(variable), Some(operation), Some(number)) = (
        captures[0].node(),
        captures[1].sym(),
        captures[2].val().and_then(|v| v.int()),
    ) else {
        return;
    };
    if !useless(operation, number) {
        return;
    }

    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        corrector.replace(
            node.source_range(),
            format!("{} = {}", variable.source(), variable.source()),
        );
    });
}

fn useless(operation: &str, number: i64) -> bool {
    if number == 0 {
        matches!(operation, "+" | "-")
    } else if number == 1 {
        matches!(operation, "*" | "/" | "**")
    } else {
        false
    }
}
