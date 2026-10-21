//! Port of `lib/rubocop/cop/lint/disjunctive_assignment_in_constructor.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Unnecessary disjunctive assignment. Use plain assignment.";

#[derive(Default)]
pub struct DisjunctiveAssignmentInConstructor;

impl Cop for DisjunctiveAssignmentInConstructor {
    fn name(&self) -> &'static str {
        "Lint/DisjunctiveAssignmentInConstructor"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        check(cx, node);
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if !node.is_method("initialize") {
        return;
    }
    check_body(cx, node.body());
}

fn check_body<'a>(cx: &mut Investigation<'a, '_>, body: Option<Node<'a>>) {
    let Some(body) = body else {
        return;
    };
    if body.is(NodeType::Begin) {
        check_body_lines(cx, body.child_nodes().collect());
    } else {
        check_body_lines(cx, vec![body]);
    }
}

fn check_body_lines<'a>(cx: &mut Investigation<'a, '_>, lines: Vec<Node<'a>>) {
    for line in lines {
        if line.is(NodeType::OrAsgn) {
            check_disjunctive_assignment(cx, line);
        } else {
            break;
        }
    }
}

fn check_disjunctive_assignment<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(lhs) = node.child_nodes().next() else {
        return;
    };
    if !lhs.is(NodeType::Ivasgn) {
        return;
    }
    let operator = node.loc().operator.unwrap();
    cx.add_offense_with_correction(operator, MSG, |corrector| {
        corrector.replace(operator, "=");
    });
}
