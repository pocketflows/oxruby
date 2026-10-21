//! Port of `lib/rubocop/cop/style/multiple_comparison.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid comparing a variable with multiple items in a conditional, use `Array#include?` instead.";

static SIMPLE_DOUBLE_COMPARISON: NodePattern = NodePattern::new("(send lvar :== lvar)");
static SIMPLE_COMPARISON_LHS: NodePattern = NodePattern::new("(send ${lvar call} :== $_)");
static SIMPLE_COMPARISON_RHS: NodePattern = NodePattern::new("(send $_ :== ${lvar call})");

#[derive(Default)]
pub struct MultipleComparison;

impl Cop for MultipleComparison {
    fn name(&self) -> &'static str {
        "Style/MultipleComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Or]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node != root_of_or_node(node) {
            return;
        }
        let allow = cx.cop_config_bool("AllowMethodComparison", true);
        if !nested_comparison(allow, node) {
            return;
        }
        let mut state = Offending::default();
        find_offending_var(allow, node, &mut state);
        let Some(variable) = state.variables.first().copied() else {
            return;
        };
        let threshold = cx.cop_config_int("ComparisonsThreshold").unwrap_or(2);
        if (state.values.len() as i64) < threshold {
            return;
        }
        let range = offense_range(&state.values);
        // Bail when an allowed method comparison sits between the collected
        // values: collapsing them into a single `include?` would drop it.
        if state.skipped.iter().any(|s| range.contains(s.source_range())) {
            return;
        }
        let replacement = preferred_method(variable, &state.values);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(range, &replacement);
        });
    }
}

#[derive(Default)]
struct Offending<'a> {
    /// A `Set` of nodes (structural equality).
    variables: Vec<Node<'a>>,
    values: Vec<Node<'a>>,
    skipped: Vec<Node<'a>>,
}

fn preferred_method(variable: Node<'_>, values: &[Node<'_>]) -> String {
    let elements: Vec<&str> = values.iter().map(|v| v.source()).collect();
    let argument = if variable.is(NodeType::Lvar) {
        variable.child_sym(0).unwrap_or("")
    } else {
        variable.source()
    };
    format!("[{}].include?({argument})", elements.join(", "))
}

fn find_offending_var<'a>(allow: bool, node: Node<'a>, state: &mut Offending<'a>) {
    if node.is(NodeType::Or) {
        find_offending_var(allow, node.lhs().unwrap(), state);
        find_offending_var(allow, node.rhs().unwrap(), state);
    } else if SIMPLE_DOUBLE_COMPARISON.is_match(node) {
    } else if let Some((var, obj)) = simple_comparison(allow, node) {
        collect_comparison(allow, node, var, obj, state);
    }
}

fn collect_comparison<'a>(
    allow: bool,
    node: Node<'a>,
    var: Node<'a>,
    obj: Node<'a>,
    state: &mut Offending<'a>,
) {
    if allow && obj.is_call() {
        state.skipped.push(node);
        return;
    }
    if !state.variables.iter().any(|v| v.same_as(var)) {
        state.variables.push(var);
    }
    if state.variables.len() > 1 {
        return;
    }
    state.values.push(obj);
}

fn offense_range(values: &[Node<'_>]) -> Range {
    let first = values.first().unwrap().parent().unwrap().source_range();
    let last = values.last().unwrap().parent().unwrap().source_range();
    first.begin().join(last.end())
}

fn nested_comparison(allow: bool, node: Node<'_>) -> bool {
    node.is(NodeType::Or) && node.child_nodes().all(|part| comparison(allow, part))
}

fn comparison(allow: bool, node: Node<'_>) -> bool {
    simple_comparison(allow, node).is_some() || nested_comparison(allow, node)
}

fn simple_comparison<'a>(allow: bool, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let (var, obj) = if let Some(c) = SIMPLE_COMPARISON_LHS.match_node(node) {
        (c[0].node()?, c[1].node()?)
    } else if let Some(c) = SIMPLE_COMPARISON_RHS.match_node(node) {
        (c[1].node()?, c[0].node()?)
    } else {
        return None;
    };
    if var.is_call() && !allow {
        return None;
    }
    Some((var, obj))
}

fn root_of_or_node(or_node: Node<'_>) -> Node<'_> {
    match or_node.parent() {
        Some(parent) if parent.is(NodeType::Or) => root_of_or_node(parent),
        _ => or_node,
    }
}
