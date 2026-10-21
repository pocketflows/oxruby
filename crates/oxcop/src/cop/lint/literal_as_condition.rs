//! Port of `lib/rubocop/cop/lint/literal_as_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{range_between, range_with_comments};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct LiteralAsCondition {
    ignored: IgnoredNodes,
}

fn message(cx: &Investigation<'_, '_>, range: Range) -> String {
    format!("Literal `{}` appeared as a condition.", cx.source(range))
}

impl Cop for LiteralAsCondition {
    fn name(&self) -> &'static str {
        "Lint/LiteralAsCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::And,
            NodeType::Or,
            NodeType::If,
            NodeType::While,
            NodeType::WhilePost,
            NodeType::Until,
            NodeType::UntilPost,
            NodeType::Case,
            NodeType::CaseMatch,
            NodeType::Send,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::And => on_and_or(cx, node, |lhs| lhs.is_truthy_literal()),
            NodeType::Or => on_and_or(cx, node, |lhs| lhs.is_falsey_literal()),
            NodeType::If => {
                let Some(cond) = condition(node) else {
                    return;
                };
                if !(cond.is_falsey_literal() || cond.is_truthy_literal()) {
                    return;
                }
                self.correct_if_node(cx, node, cond);
            }
            NodeType::While => on_loop(cx, node, true),
            NodeType::Until => on_loop(cx, node, false),
            NodeType::WhilePost => on_post_loop(cx, node, true),
            NodeType::UntilPost => on_post_loop(cx, node, false),
            NodeType::Case => on_case(cx, node),
            NodeType::CaseMatch => on_case_match(cx, node),
            _ => {
                if !node.is_negation_method() {
                    return;
                }
                check_for_literal(cx, node);
            }
        }
    }
}

fn on_and_or<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, offending: fn(Node<'_>) -> bool) {
    let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
        return;
    };
    if !offending(lhs) {
        return;
    }
    let message = message(cx, lhs.source_range());
    cx.add_offense_with_correction(lhs.source_range(), &message, |corrector| {
        if void_value_expression(Some(rhs)) {
            return;
        }
        corrector.replace(node.source_range(), rhs.source());
    });
}

/// `on_while` (`truthy` is true) and `on_until`.
fn on_loop<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, while_loop: bool) {
    let condition = node.condition().unwrap();
    let keep = if while_loop { "true" } else { "false" };
    if condition.source() == keep {
        return;
    }
    let (replaces, removes) = if while_loop {
        (condition.is_truthy_literal(), condition.is_falsey_literal())
    } else {
        (condition.is_falsey_literal(), condition.is_truthy_literal())
    };
    let message = message(cx, condition.source_range());
    if replaces {
        cx.add_offense_with_correction(condition.source_range(), &message, |corrector| {
            corrector.replace(condition.source_range(), keep);
        });
    } else if removes {
        cx.add_offense_with_correction(condition.source_range(), &message, |corrector| {
            corrector.remove(node.source_range());
        });
    }
}

/// `on_while_post` and `on_until_post`.
fn on_post_loop<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, while_loop: bool) {
    let condition = node.condition().unwrap();
    let keep = if while_loop { "true" } else { "false" };
    if condition.source() == keep {
        return;
    }
    let (replaces, unrolls) = if while_loop {
        (condition.is_truthy_literal(), condition.is_falsey_literal())
    } else {
        (condition.is_falsey_literal(), condition.is_truthy_literal())
    };
    let message = message(cx, condition.source_range());
    if replaces {
        cx.add_offense_with_correction(condition.source_range(), &message, |corrector| {
            corrector.replace(
                node.source_range(),
                node.source().replacen(condition.source(), keep, 1),
            );
        });
    } else if unrolls {
        cx.add_offense_with_correction(condition.source_range(), &message, |corrector| {
            let body: Vec<&str> = node
                .body()
                .map(|b| b.child_nodes().map(|c| c.source()).collect())
                .unwrap_or_default();
            corrector.replace(node.source_range(), body.join("\n"));
        });
    }
}

fn on_case<'a>(cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
    if let Some(cond) = case_node.condition() {
        if !cond.is_falsey_literal() && !cond.is_truthy_literal() {
            return;
        }
        check_case(cx, case_node);
    } else {
        for when_node in case_node.when_branches() {
            if !when_node.conditions().iter().all(|c| c.is_literal()) {
                continue;
            }
            let range = when_conditions_range(when_node);
            let message = message(cx, range);
            cx.add_offense(range, &message);
        }
    }
}

fn on_case_match<'a>(cx: &mut Investigation<'a, '_>, case_match_node: Node<'a>) {
    if case_match_node.condition().is_some() {
        if case_match_node
            .descendants()
            .any(|d| d.is(NodeType::MatchVar))
        {
            return;
        }
        check_case(cx, case_match_node);
    } else {
        for in_pattern_node in case_match_node.when_branches() {
            let Some(condition) = in_pattern_node.child(0) else {
                continue;
            };
            if !condition.is_literal() {
                continue;
            }
            let message = message(cx, in_pattern_node.source_range());
            cx.add_offense(in_pattern_node.source_range(), &message);
        }
    }
}

fn void_value_expression(node: Option<Node<'_>>) -> bool {
    let mut node = node;
    while let Some(n) = node.filter(|n| n.is(NodeType::Begin)) {
        node = n.child_from_end(1);
    }
    node.is_some_and(|n| n.is_any(&[NodeType::Return, NodeType::Break, NodeType::Next]))
}

fn check_for_literal<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(cond) = condition(node) else {
        return;
    };
    if cond.is_literal() {
        let message = message(cx, cond.source_range());
        cx.add_offense(cond.source_range(), &message);
    } else {
        check_node(cx, cond);
    }
}

fn basic_literal(node: Node<'_>) -> bool {
    if node.is(NodeType::Array) {
        primitive_array(node)
    } else {
        node.is_basic_literal()
    }
}

fn primitive_array(node: Node<'_>) -> bool {
    node.child_nodes().all(basic_literal)
}

fn check_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.is_send() && node.is_prefix_bang() {
        if let Some(receiver) = node.receiver() {
            handle_node(cx, receiver);
        }
    } else if node.is_operator_keyword() {
        for op in node.child_nodes() {
            handle_node(cx, op);
        }
    } else if node.is(NodeType::Begin)
        && node.children().len() == 1
        && let Some(first) = node.child(0)
    {
        handle_node(cx, first);
    }
}

fn handle_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.is_literal() {
        if node.parent().is_some_and(|p| p.is(NodeType::And)) {
            return;
        }
        let message = message(cx, node.source_range());
        cx.add_offense(node.source_range(), &message);
    } else if node.is_any(&[NodeType::Send, NodeType::And, NodeType::Or, NodeType::Begin]) {
        check_node(cx, node);
    }
}

fn check_case<'a>(cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
    let condition = case_node.condition().unwrap();
    if condition.is(NodeType::Array) && !primitive_array(condition) {
        return;
    }
    if condition.is(NodeType::Dstr) {
        return;
    }
    handle_node(cx, condition);
}

fn condition(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_send() {
        node.receiver()
    } else {
        node.condition()
    }
}

fn when_conditions_range(when_node: Node<'_>) -> Range {
    let conditions = when_node.conditions();
    range_between(
        conditions.first().unwrap().source_range().begin_pos,
        conditions.last().unwrap().source_range().end_pos,
    )
}

fn condition_evaluation(node: Node<'_>, cond: Node<'_>) -> bool {
    if node.is_unless() {
        cond.is_falsey_literal()
    } else {
        cond.is_truthy_literal()
    }
}

impl LiteralAsCondition {
    fn correct_if_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, cond: Node<'a>) {
        let result = condition_evaluation(node, cond);
        let surviving_branch = if result {
            node.if_branch()
        } else {
            node.else_branch()
        };
        if surviving_branch.is_none() && (node.is_elsif() || node.has_else()) {
            return;
        }
        let new_node = if node.is_elsif() && result {
            let range = range_with_comments(cx.processed_source, node.if_branch().unwrap());
            format!("else\n  {}", cx.source(range))
        } else if node.is_elsif() && !result {
            format!("else\n  {}", node.else_branch().unwrap().source())
        } else if let Some(if_branch) = node.if_branch().filter(|_| result) {
            if_branch.source().to_owned()
        } else if node.is_elsif_conditional() {
            format!(
                "{}\nend",
                node.else_branch().unwrap().source().replacen("elsif", "if", 1)
            )
        } else if node.has_else() || node.is_ternary() {
            node.else_branch()
                .map(|e| e.source().to_owned())
                .unwrap_or_default()
        } else {
            // Equivalent to removing the node
            String::new()
        };
        let message = message(cx, cond.source_range());
        let ignored = &mut self.ignored;
        cx.add_offense_with_correction(cond.source_range(), &message, |corrector| {
            if ignored.part_of(node) {
                return;
            }
            corrector.replace(node.source_range(), new_node);
            ignored.ignore(node);
        });
    }
}
