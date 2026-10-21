//! Port of `lib/rubocop/cop/style/redundant_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Redundant assignment before returning detected.";

#[derive(Default)]
pub struct RedundantAssignment;

impl Cop for RedundantAssignment {
    fn name(&self) -> &'static str {
        "Style/RedundantAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        check_branch(cx, node.body());
    }
}

fn check_branch<'a>(cx: &mut Investigation<'a, '_>, node: Option<Node<'a>>) {
    let Some(node) = node else {
        return;
    };
    match node.ty() {
        NodeType::Case | NodeType::CaseMatch => {
            for when_node in node.when_branches() {
                check_branch(cx, when_node.body());
            }
            check_branch(cx, node.else_branch());
        }
        NodeType::If => {
            if node.is_modifier_form() || node.is_ternary() {
                return;
            }
            check_branch(cx, node.if_branch());
            check_branch(cx, node.else_branch());
        }
        NodeType::Rescue | NodeType::Resbody => {
            for child in node.child_nodes() {
                check_branch(cx, Some(child));
            }
        }
        NodeType::Ensure => check_branch(cx, node.ensure_branch()),
        NodeType::Begin | NodeType::Kwbegin => check_begin_node(cx, node),
        _ => {}
    }
}

/// `(... $(lvasgn _name _expression) (lvar _name))`
fn redundant_assignment(node: Node<'_>) -> Option<Node<'_>> {
    let children = node.children();
    if children.len() < 2 {
        return None;
    }
    let last = node.child_from_end(1)?;
    let assignment = node.child_from_end(2)?;
    (assignment.is(NodeType::Lvasgn)
        && assignment.child(1).is_some()
        && last.is(NodeType::Lvar)
        && last.child_sym(0) == assignment.child_sym(0))
    .then_some(assignment)
}

fn check_begin_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if let Some(assignment) = redundant_assignment(node) {
        let ps = cx.processed_source;
        cx.add_offense_with_correction(assignment.source_range(), MSG, |corrector| {
            let reference = assignment.right_sibling().unwrap();
            let first = assignment.first_line();
            let last = ps.line_of(reference.source_range().begin_pos);
            if !ps.comments_in_lines(first, last).is_empty() {
                return;
            }
            let expression = assignment.child(1).unwrap();
            corrector.replace(assignment.source_range(), expression.source());
            corrector.remove(reference.source_range());
        });
    } else {
        check_branch(cx, node.child_nodes().last());
    }
}
