//! Port of `lib/rubocop/cop/style/or_assignment.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use the double pipe equals operator `||=` instead.";

static TERNARY_ASSIGNMENT: NodePattern = NodePattern::new(
    "({lvasgn ivasgn cvasgn gvasgn} _var
       (if
         ({lvar ivar cvar gvar} _var)
         ({lvar ivar cvar gvar} _var)
         $_))",
);

static UNLESS_ASSIGNMENT: NodePattern = NodePattern::new(
    "(if
       ({lvar ivar cvar gvar} _var) nil?
       ({lvasgn ivasgn cvasgn gvasgn} _var
         _))",
);

#[derive(Default)]
pub struct OrAssignment;

impl Cop for OrAssignment {
    fn name(&self) -> &'static str {
        "Style/OrAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::If,
            NodeType::Lvasgn,
            NodeType::Ivasgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::If) {
            if !UNLESS_ASSIGNMENT.is_match(node) {
                return;
            }
        } else {
            let Some(captures) = TERNARY_ASSIGNMENT.match_node(node) else {
                return;
            };
            if captures[0].node().is_some_and(|e| e.is(NodeType::If)) {
                return;
            }
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let (variable, default) = if TERNARY_ASSIGNMENT.is_match(node) {
                (
                    node.name().unwrap(),
                    node.expression().unwrap().else_branch().unwrap(),
                )
            } else {
                let assignment = node.if_branch().or_else(|| node.else_branch()).unwrap();
                (assignment.name().unwrap(), assignment.expression().unwrap())
            };
            corrector.replace(
                node.source_range(),
                format!("{variable} ||= {}", default.source()),
            );
        });
    }
}
