//! Port of `lib/rubocop/cop/style/fetch_env_var.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static ENV_WITH_BRACKET: NodePattern = NodePattern::new("(send (const nil? :ENV) :[] $_)");

#[derive(Default)]
pub struct FetchEnvVar;

impl Cop for FetchEnvVar {
    fn name(&self) -> &'static str {
        "Style/FetchEnvVar"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(name_node) = ENV_WITH_BRACKET.match_node(node).and_then(|c| c[0].node()) else {
            return;
        };
        if !offensive(cx, node) {
            return;
        }
        let default_to_nil = cx.cop_config_bool("DefaultToNil", true);
        let key = name_node.source();
        let message = if default_to_nil {
            format!("Use `ENV.fetch({key}, nil)` instead of `ENV[{key}]`.")
        } else {
            format!("Use `ENV.fetch({key})` instead of `ENV[{key}]`.")
        };
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let code = if default_to_nil {
                format!("ENV.fetch({key}, nil)")
            } else {
                format!("ENV.fetch({key})")
            };
            corrector.replace(node.source_range(), code);
        });
    }
}

fn same_nodes(a: &[Node<'_>], b: &[Node<'_>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_as(*y))
}

fn allowed_var(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(env_key_node) = node.child_nodes().last() else {
        return false;
    };
    env_key_node.is(NodeType::Str)
        && env_key_node
            .str_content()
            .is_some_and(|v| cx.cop_config_list("AllowedVariables").contains(&v))
}

fn used_as_flag(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if used_if_condition_in_body(node) {
        return true;
    }
    parent.is(NodeType::Send) && (parent.is_prefix_bang() || parent.is_comparison_method())
}

fn used_if_condition_in_body(node: Node<'_>) -> bool {
    let Some(if_node) = node.ancestors().find(|a| a.is(NodeType::If)) else {
        return false;
    };
    let Some(condition) = if_node.condition() else {
        return false;
    };
    let node_children: Vec<_> = node.child_nodes().collect();
    if condition.is(NodeType::Send)
        && same_nodes(&condition.child_nodes().collect::<Vec<_>>(), &node_children)
    {
        return true;
    }
    used_in_condition(node, condition)
}

fn used_in_condition(node: Node<'_>, condition: Node<'_>) -> bool {
    if condition.is(NodeType::Send) {
        if condition.is_assignment_method() && partial_matched(node, condition) {
            return true;
        }
        if !condition.is_comparison_method() && !condition.is_predicate_method() {
            return false;
        }
    }
    condition.child_nodes().any(|c| c.same_as(node))
}

/// `node.child_nodes == node.child_nodes & condition.child_nodes`
fn partial_matched(node: Node<'_>, condition: Node<'_>) -> bool {
    let a: Vec<_> = node.child_nodes().collect();
    let b: Vec<_> = condition.child_nodes().collect();
    let mut intersection: Vec<Node<'_>> = Vec::new();
    for x in &a {
        if b.iter().any(|y| y.same_as(*x)) && !intersection.iter().any(|y| y.same_as(*x)) {
            intersection.push(*x);
        }
    }
    same_nodes(&a, &intersection)
}

fn offensive(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !(allowed_var(cx, node) || allowable_use(node))
}

fn message_chained_with_dot(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !parent.is_call() || parent.child(0) != Some(node) {
        return false;
    }
    parent.is_dot() || parent.is_safe_navigation()
}

fn allowable_use(node: Node<'_>) -> bool {
    used_as_flag(node) || message_chained_with_dot(node) || assigned(node) || or_lhs(node)
}

fn assigned(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let assignment = if parent.is_call() {
        parent.is_setter_method()
    } else {
        parent.is_assignment()
    };
    assignment && parent.child(0).is_some_and(|lhs| lhs.same_as(node))
}

fn or_lhs(node: Node<'_>) -> bool {
    let Some(parent) = node.parent().filter(|p| p.is(NodeType::Or)) else {
        return false;
    };
    parent.lhs() == Some(node) || parent.parent().is_some_and(|p| p.is(NodeType::Or))
}
