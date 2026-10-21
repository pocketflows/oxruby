//! Port of `lib/rubocop/cop/style/method_call_without_args_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use parentheses for method calls with no arguments.";

#[derive(Default)]
pub struct MethodCallWithoutArgsParentheses;

impl Cop for MethodCallWithoutArgsParentheses {
    fn name(&self) -> &'static str {
        "Style/MethodCallWithoutArgsParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_arguments() || !node.is_parenthesized() {
            return;
        }
        if ineligible_node(node) || default_argument(node) {
            return;
        }
        let name = node.method_name().unwrap_or("");
        if allowed_method(cx, name) || matches_allowed_pattern(cx, name) {
            return;
        }
        if same_name_assignment(node) || parenthesized_it_method_in_block(node) {
            return;
        }
        let range = node.loc().begin.unwrap().join(node.loc().end.unwrap());
        if cx.processed_source.contains_comment(range) {
            return;
        }
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove(range);
        });
    }
}

fn ineligible_node(node: Node<'_>) -> bool {
    node.is_camel_case_method() || node.is_implicit_call() || node.is_prefix_not()
}

fn default_argument(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is(NodeType::Optarg))
}

fn same_name_assignment(node: Node<'_>) -> bool {
    if node.receiver().is_some() {
        return false;
    }
    let method_name = node.method_name().unwrap_or("");
    node.ancestors()
        .filter(|a| a.is_assignment())
        // `obj.method ||= value` and `obj.method += value` are assignments
        // whose `lhs` is a call.
        .filter(|a| !(a.is_shorthand_asgn() && a.lhs().is_some_and(|l| l.is_call())))
        .any(|asgn_node| {
            if asgn_node.is(NodeType::Masgn) {
                variable_in_mass_assignment(method_name, asgn_node)
            } else {
                asgn_node
                    .loc()
                    .name
                    .is_some_and(|name| asgn_node.range_source(name) == method_name)
            }
        })
}

/// Respects `Lint/ItWithoutArgumentsInBlock` and Ruby 3.3's warning about
/// `it` calls without arguments in blocks.
fn parenthesized_it_method_in_block(node: Node<'_>) -> bool {
    if !node.is_method("it") {
        return false;
    }
    let Some(block_node) = node.ancestors().find(|a| a.is_any_block()) else {
        return false;
    };
    // Inside a numbered/`it` block, a bare `it` is a parse error, so `it()`
    // must keep its parentheses.
    if block_node.is(NodeType::Block)
        && block_node
            .child(1)
            .is_some_and(|args| args.loc().expression.is_some())
    {
        return false;
    }
    node.receiver().is_none() && node.arguments().is_empty() && !node.is_block_literal()
}

/// `MlhsNode#assignments`.
fn mlhs_assignments<'a>(mlhs: Node<'a>, out: &mut Vec<Node<'a>>) {
    for child in mlhs.child_nodes() {
        if child.is(NodeType::Splat) {
            out.extend(child.child_nodes());
        } else if child.is(NodeType::Mlhs) {
            mlhs_assignments(child, out);
        } else {
            out.push(child);
        }
    }
}

fn variable_in_mass_assignment(variable_name: &str, node: Node<'_>) -> bool {
    let mut assignments = Vec::new();
    if let Some(mlhs) = node.child(0) {
        mlhs_assignments(mlhs, &mut assignments);
    }
    assignments
        .iter()
        .filter(|n| !n.is(NodeType::Send))
        .any(|n| n.name() == Some(variable_name))
}
