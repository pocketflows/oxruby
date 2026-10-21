//! Port of `lib/rubocop/cop/naming/predicate_method.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Investigation};

const MSG_PREDICATE: &str = "Predicate method names should end with `?`.";
const MSG_NON_PREDICATE: &str = "Non-predicate method names should not end with `?`.";

/// A return value: a node, or one of the `s(:nil)` / `s(:array)` sexps
/// upstream synthesizes for implicit `nil` and multiple return values.
#[derive(Clone, Copy)]
enum Value<'a> {
    Node(Node<'a>),
    Nil,
    Array,
}

impl<'a> Value<'a> {
    fn node(self) -> Option<Node<'a>> {
        match self {
            Value::Node(n) => Some(n),
            _ => None,
        }
    }

    fn is_super(self) -> bool {
        self.node()
            .is_some_and(|n| matches!(n.ty(), NodeType::Super | NodeType::Zsuper))
    }

    fn is_call(self) -> bool {
        self.node().is_some_and(|n| n.is_call())
    }

    fn is_boolean(self) -> bool {
        self.node().is_some_and(|n| n.is_boolean())
    }

    fn is_literal(self) -> bool {
        match self {
            Value::Node(n) => n.is_literal(),
            Value::Nil | Value::Array => true,
        }
    }

    fn is_return(self) -> bool {
        self.node().is_some_and(|n| n.is(NodeType::Return))
    }
}

#[derive(Default)]
pub struct PredicateMethod;

impl Cop for PredicateMethod {
    fn name(&self) -> &'static str {
        "Naming/PredicateMethod"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if allowed(cx, node) {
            return;
        }
        let return_values = return_values(node.body().unwrap());
        if acceptable(cx, &return_values) {
            return;
        }
        if node.is_predicate_method() && potential_non_predicate(cx, &return_values) {
            cx.add_offense(node.loc().name.unwrap(), MSG_NON_PREDICATE);
        } else if !node.is_predicate_method() && all_return_values_boolean(cx, &return_values) {
            cx.add_offense(node.loc().name.unwrap(), MSG_PREDICATE);
        }
    }
}

fn allowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let method_name = node.method_name().unwrap_or("");
    node.is_method("initialize")
        || allowed_method(cx, method_name)
        || matches_allowed_pattern(cx, method_name)
        || allowed_bang_method(cx, node)
        || node.is_operator_method()
        || node.body().is_none()
}

/// In `conservative` mode, if the method returns `super`, `zsuper`, or a
/// non-comparison method call, the method name is acceptable.
fn acceptable(cx: &Investigation<'_, '_>, return_values: &[Value<'_>]) -> bool {
    if !conservative(cx) {
        return false;
    }
    return_values
        .iter()
        .any(|value| value.is_super() || unknown_method_call(cx, *value))
}

fn unknown_method_call(cx: &Investigation<'_, '_>, value: Value<'_>) -> bool {
    value.is_call() && !method_returning_boolean(cx, value)
}

/// Collect all the (implicit and explicit) return values of a node.
fn return_values(node: Node<'_>) -> Vec<Value<'_>> {
    let mut return_values = Vec::new();
    if !node.is(NodeType::Begin) {
        return_values.push(extract_return_value(Value::Node(node)));
    }
    for return_node in node.descendants().filter(|d| d.is(NodeType::Return)) {
        return_values.push(extract_return_value(Value::Node(return_node)));
    }
    return_values.push(last_value(Value::Node(node)));
    process_return_values(return_values)
}

fn all_return_values_boolean(cx: &Investigation<'_, '_>, return_values: &[Value<'_>]) -> bool {
    let values: Vec<&Value<'_>> = return_values.iter().filter(|v| !v.is_super()).collect();
    if values.is_empty() {
        return false;
    }
    values.iter().all(|value| boolean_return(cx, **value))
}

fn boolean_return(cx: &Investigation<'_, '_>, value: Value<'_>) -> bool {
    value.is_boolean() || method_returning_boolean(cx, value)
}

fn method_returning_boolean(cx: &Investigation<'_, '_>, value: Value<'_>) -> bool {
    let Some(value) = value.node().filter(|n| n.is_call()) else {
        return false;
    };
    if wayward_predicate(cx, value.method_name().unwrap_or("")) {
        return false;
    }
    value.is_comparison_method() || value.is_predicate_method() || value.is_negation_method()
}

/// Assumes a method to be non-predicate if all return values are
/// non-boolean literals.
///
/// In `Mode: conservative`, if any of the return values is a boolean, the
/// method name is acceptable. In `Mode: aggressive`, all return values must
/// be booleans for a predicate method, or else an offense will be
/// registered.
fn potential_non_predicate(cx: &Investigation<'_, '_>, return_values: &[Value<'_>]) -> bool {
    if conservative(cx) && return_values.iter().any(|v| boolean_return(cx, *v)) {
        return false;
    }
    return_values
        .iter()
        .any(|value| value.is_literal() && !value.is_boolean())
}

fn extract_return_value(value: Value<'_>) -> Value<'_> {
    let Some(node) = value.node().filter(|n| n.is(NodeType::Return)) else {
        return value;
    };
    let arguments = node.arguments();
    match arguments.as_slice() {
        // `return` without a value is a `nil` return.
        [] => Value::Nil,
        [one] => Value::Node(*one),
        // When there's a multiple return, it cannot be a predicate so just
        // return an `array` sexp for simplicity.
        _ => Value::Array,
    }
}

fn last_value(value: Value<'_>) -> Value<'_> {
    let value = match value.node() {
        Some(node) if node.is(NodeType::Begin) => {
            node.child_from_end(1).map_or(Value::Nil, Value::Node)
        }
        _ => value,
    };
    if value.is_return() {
        extract_return_value(value)
    } else {
        value
    }
}

fn process_return_values(return_values: Vec<Value<'_>>) -> Vec<Value<'_>> {
    return_values
        .into_iter()
        .flat_map(|value| match value.node() {
            Some(node) if node.is_conditional() => {
                process_return_values(extract_conditional_branches(node))
            }
            Some(node) if and_or(node) => process_return_values(extract_and_or_clauses(node)),
            _ => vec![value],
        })
        .collect()
}

fn and_or(node: Node<'_>) -> bool {
    matches!(node.ty(), NodeType::And | NodeType::Or)
}

/// Recursively traverse an `and` or `or` node to collect all clauses
/// within.
fn extract_and_or_clauses(node: Node<'_>) -> Vec<Value<'_>> {
    if !and_or(node) {
        return vec![Value::Node(node)];
    }
    let mut clauses = extract_and_or_clauses(node.lhs().unwrap());
    clauses.extend(extract_and_or_clauses(node.rhs().unwrap()));
    clauses
}

fn extract_conditional_branches(node: Node<'_>) -> Vec<Value<'_>> {
    if matches!(node.ty(), NodeType::While | NodeType::Until) {
        // If there is no body, act as implicit `nil`.
        return vec![node.body().map_or(Value::Nil, |b| last_value(Value::Node(b)))];
    }
    // Branches with no value act as an implicit `nil`.
    let mut branches: Vec<Value<'_>> = node
        .branches()
        .into_iter()
        .map(|branch| branch.map_or(Value::Nil, |b| last_value(Value::Node(b))))
        .collect();
    // Missing else branches also act as an implicit `nil`.
    if node.else_branch().is_none() {
        branches.push(Value::Nil);
    }
    branches
}

fn conservative(cx: &Investigation<'_, '_>) -> bool {
    // `fetch('Mode', :conservative).to_sym`; a symbol in the spec's config
    // reaches us as `:conservative`.
    let mode = cx.cop_config_str("Mode").unwrap_or("conservative");
    mode.strip_prefix(':').unwrap_or(mode) == "conservative"
}

fn allowed_bang_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowBangMethods", false) && node.is_bang_method()
}

/// If a method ending in `?` is known to not return a boolean value (for
/// example, `Numeric#nonzero?`) it should be treated as a non-boolean
/// value, despite the method naming.
fn wayward_predicate(cx: &Investigation<'_, '_>, name: &str) -> bool {
    cx.cop_config_list("WaywardPredicates").contains(&name)
}
