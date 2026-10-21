//! Port of `lib/rubocop/cop/lint/float_comparison.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG_EQUALITY: &str = "Avoid equality comparisons of floats as they are unreliable.";
const MSG_INEQUALITY: &str = "Avoid inequality comparisons of floats as they are unreliable.";
const MSG_CASE: &str = "Avoid float literal comparisons in case statements as they are unreliable.";

const FLOAT_RETURNING_METHODS: &[&str] = &["to_f", "Float", "fdiv"];
const FLOAT_INSTANCE_METHODS: &[&str] = &[
    "@-",
    "abs",
    "magnitude",
    "modulo",
    "next_float",
    "prev_float",
    "quo",
];

#[derive(Default)]
pub struct FloatComparison;

impl Cop for FloatComparison {
    fn name(&self) -> &'static str {
        "Lint/FloatComparison"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Case]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "!=", "eql?", "equal?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Case) {
            on_case(cx, node);
            return;
        }
        if node.arguments().len() != 1 {
            return;
        }
        let lhs = node.receiver();
        let rhs = node.first_argument();
        if literal_safe(lhs) || literal_safe(rhs) {
            return;
        }
        let message = if node.is_method("!=") {
            MSG_INEQUALITY
        } else {
            MSG_EQUALITY
        };
        if is_float(lhs) || is_float(rhs) {
            cx.add_offense(node.source_range(), message);
        }
    }
}

fn on_case<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    for when_branch in node.when_branches() {
        for condition in when_branch.conditions() {
            if !is_float(Some(condition)) || literal_safe(Some(condition)) {
                continue;
            }
            cx.add_offense(condition.source_range(), MSG_CASE);
        }
    }
}

fn is_float(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    match node.ty() {
        NodeType::Float => true,
        NodeType::Send => float_send(node),
        NodeType::Begin => is_float(node.child(0)),
        _ => false,
    }
}

fn literal_safe(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if node.is(NodeType::Begin) {
        return literal_safe(node.child(0));
    }
    (node.is_numeric() && node.value().is_some_and(is_zero)) || node.is(NodeType::Nil)
}

fn is_zero(value: &Child) -> bool {
    match value {
        Child::Int(i) => *i == 0,
        Child::Float(f) => *f == 0.0,
        Child::Rational(n, _) => n == "0",
        Child::Complex(imaginary) => is_zero(imaginary),
        _ => false,
    }
}

fn float_send(node: Node<'_>) -> bool {
    if node.is_arithmetic_operation() {
        is_float(node.receiver()) || is_float(node.first_argument())
    } else if node
        .method_name()
        .is_some_and(|m| FLOAT_RETURNING_METHODS.contains(&m))
    {
        true
    } else if node.receiver().is_some_and(|r| r.is(NodeType::Float)) {
        node.method_name()
            .is_some_and(|m| FLOAT_INSTANCE_METHODS.contains(&m))
            || numeric_returning_method(node)
    } else {
        false
    }
}

fn numeric_returning_method(node: Node<'_>) -> bool {
    let Some(receiver) = node.receiver() else {
        return false;
    };
    match node.method_name().unwrap_or_default() {
        "angle" | "arg" | "phase" => receiver
            .source()
            .replace('_', "")
            .parse::<f64>()
            .is_ok_and(|f| f < 0.0),
        "ceil" | "floor" | "round" | "truncate" => {
            let Some(precision) = node.first_argument() else {
                return false;
            };
            precision.is(NodeType::Int)
                && match precision.value() {
                    Some(Child::Int(i)) => *i > 0,
                    Some(Child::BigInt(s)) => !s.starts_with('-'),
                    _ => false,
                }
        }
        _ => false,
    }
}
