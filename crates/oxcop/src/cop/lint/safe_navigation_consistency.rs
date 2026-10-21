//! Port of `lib/rubocop/cop/lint/safe_navigation_consistency.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::nil_methods::nil_method;
use crate::cop::{Cop, Investigation};

const USE_DOT_MSG: &str = "Use `.` instead of unnecessary `&.`.";
const USE_SAFE_NAVIGATION_MSG: &str = "Use `&.` for consistency with safe navigation.";

#[derive(Default)]
pub struct SafeNavigationConsistency;

impl Cop for SafeNavigationConsistency {
    fn name(&self) -> &'static str {
        "Lint/SafeNavigationConsistency"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::And, NodeType::Or]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let mut all_operands = Vec::new();
        collect_operands(node, &mut all_operands);

        // `group_by { receiver_name_as_key }`, in order of first appearance.
        let mut operand_groups: Vec<(String, Vec<Node<'a>>)> = Vec::new();
        for operand in all_operands {
            let key = receiver_name_as_key(operand);
            match operand_groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, group)) => group.push(operand),
                None => operand_groups.push((key, vec![operand])),
            }
        }

        for (_, grouped_operands) in operand_groups {
            let Some((dot_op, begin_of_rest_operands)) =
                find_consistent_parts(cx, &grouped_operands)
            else {
                continue;
            };
            for &operand in grouped_operands.iter().skip(begin_of_rest_operands) {
                if already_appropriate_call(operand, dot_op) {
                    continue;
                }
                register_offense(cx, operand, dot_op);
            }
        }
    }
}

fn collect_operands<'a>(node: Node<'a>, operand_nodes: &mut Vec<Node<'a>>) {
    if let Some(lhs) = node.lhs() {
        push_operand_nodes(lhs, operand_nodes);
    }
    if let Some(rhs) = node.rhs() {
        push_operand_nodes(rhs, operand_nodes);
    }
}

/// `receiver_name_as_key`. (Operands' parents are always `and`/`or`, so
/// the `method.parent.call_type?` branch, which calls an undefined
/// `receiver` in RuboCop, is never taken.)
fn receiver_name_as_key(method: Node<'_>) -> String {
    method
        .receiver()
        .map(|r| r.source().to_owned())
        .unwrap_or_default()
}

fn find_consistent_parts(
    cx: &Investigation<'_, '_>,
    grouped_operands: &[Node<'_>],
) -> Option<(&'static str, usize)> {
    let (csend_in_and, csend_in_or, send_in_and, send_in_or) =
        most_left_indices(cx, grouped_operands);

    if let (Some(a), Some(o)) = (csend_in_and, csend_in_or)
        && a < o
    {
        return None;
    }

    if let Some(csend_in_and) = csend_in_and {
        let start = match send_in_and {
            Some(send_in_and) => send_in_and.min(csend_in_and),
            None => csend_in_and,
        };
        Some((".", start + 1))
    } else if let (Some(send_in_or), Some(csend_in_or)) = (send_in_or, csend_in_or) {
        if send_in_or < csend_in_or {
            Some((".", send_in_or + 1))
        } else {
            Some(("&.", csend_in_or + 1))
        }
    } else if let (Some(send_in_and), Some(csend_in_or)) = (send_in_and, csend_in_or)
        && send_in_and < csend_in_or
    {
        Some((".", csend_in_or))
    } else {
        None
    }
}

fn already_appropriate_call(operand: Node<'_>, dot_op: &str) -> bool {
    if operand.is_safe_navigation() && dot_op == "&." {
        return true;
    }
    (operand.is_dot() || operand.is_operator_method()) && dot_op == "."
}

fn register_offense(cx: &mut Investigation<'_, '_>, operand: Node<'_>, dot_operator: &str) {
    let offense_range = if operand.is_operator_method() {
        Some(operand.source_range())
    } else {
        operand.loc().dot
    };
    let Some(offense_range) = offense_range else {
        return;
    };
    let message = if dot_operator == "." {
        USE_DOT_MSG
    } else {
        USE_SAFE_NAVIGATION_MSG
    };
    let operator_method = operand.is_operator_method();
    let dot = operand.loc().dot;
    cx.add_offense_with_correction(offense_range, message, |corrector| {
        if operator_method {
            return;
        }
        if let Some(dot) = dot {
            corrector.replace(dot, dot_operator);
        }
    });
}

fn push_operand_nodes<'a>(operand: Node<'a>, operand_nodes: &mut Vec<Node<'a>>) {
    if operand.is_operator_keyword() {
        collect_operands(operand, operand_nodes);
    } else if operand.is_call() {
        operand_nodes.push(operand);
    }
}

type Indices = (Option<usize>, Option<usize>, Option<usize>, Option<usize>);

fn most_left_indices(cx: &Investigation<'_, '_>, grouped_operands: &[Node<'_>]) -> Indices {
    let (mut csend_in_and, mut csend_in_or, mut send_in_and, mut send_in_or) =
        (None, None, None, None);
    for (index, &operand) in grouped_operands.iter().enumerate() {
        let in_and = operand_in(operand, NodeType::And);
        let in_or = operand_in(operand, NodeType::Or);
        let csend = operand.is(NodeType::Csend);
        if in_and && csend {
            csend_in_and.get_or_insert(index);
        }
        if in_or && csend {
            csend_in_or.get_or_insert(index);
        }
        if in_and && !nilable(cx, operand) {
            send_in_and.get_or_insert(index);
        }
        if in_or && !nilable(cx, operand) {
            send_in_or.get_or_insert(index);
        }
    }
    (csend_in_and, csend_in_or, send_in_and, send_in_or)
}

/// `operand_in_and?` / `operand_in_or?`.
fn operand_in(node: Node<'_>, ty: NodeType) -> bool {
    node.parent().is_some_and(|p| p.is(ty))
}

fn nilable(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is(NodeType::Csend) || nil_method(cx, node.method_name().unwrap_or_default())
}
