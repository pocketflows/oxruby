//! Port of `lib/rubocop/cop/lint/safe_navigation_chain.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::nil_methods::nil_method;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Do not chain ordinary method call after safe navigation operator.";
const PLUS_MINUS_METHODS: &[&str] = &["+@", "-@"];

static BAD_METHOD: NodePattern = NodePattern::new(
    "{
       (send $(csend ...) $_ ...)
       (send $(any_block (csend ...) ...) $_ ...)
       (send $(begin (csend ...)) $_ ...)
     }",
);

#[derive(Default)]
pub struct SafeNavigationChain;

impl Cop for SafeNavigationChain {
    fn name(&self) -> &'static str {
        "Lint/SafeNavigationChain"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.3)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !require_safe_navigation(node) {
            return;
        }
        let Some(captures) = BAD_METHOD.match_node(node) else {
            return;
        };
        let (Some(safe_nav), Some(method)) = (captures[0].node(), captures[1].sym()) else {
            return;
        };
        if nil_method(cx, method)
            || node
                .method_name()
                .is_some_and(|m| PLUS_MINUS_METHODS.contains(&m))
        {
            return;
        }
        if ternary_safe_navigation(node, safe_nav) {
            return;
        }

        let begin_range = node
            .loc()
            .dot
            .unwrap_or_else(|| safe_nav.source_range().end());
        let location = begin_range.join(node.source_range().end());
        let else_branch = ternary_else_branch(node, safe_nav);
        let offense_source = cx.source(location).to_owned();
        cx.add_offense_with_correction(location, MSG, |corrector| {
            if else_branch {
                return;
            }
            autocorrect(corrector, location, &offense_source, node);
        });
    }
}

fn require_safe_navigation(node: Node<'_>) -> bool {
    let Some(parent) = node.parent().filter(|p| p.is(NodeType::And)) else {
        return true;
    };
    let (Some(lhs), Some(rhs)) = (parent.lhs(), parent.rhs()) else {
        return true;
    };
    !rhs.same_as(node) || !same_option(lhs.receiver(), rhs.receiver())
}

fn same_option(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
    }
}

fn ternary_safe_navigation(node: Node<'_>, safe_nav: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::If)
        && parent.if_branch() == Some(node)
        && parent.condition().is_some_and(|c| c.same_as(safe_nav))
}

fn ternary_else_branch(node: Node<'_>, safe_nav: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::If)
        && parent.else_branch() == Some(node)
        && parent.condition().is_some_and(|c| c.same_as(safe_nav))
}

fn add_safe_navigation_operator(offense_source: &str, send_node: Node<'_>) -> String {
    let mut source = if brackets(send_node) {
        let arguments: Vec<&str> = send_node.arguments().iter().map(|a| a.source()).collect();
        // `method_chain` is the source of an empty range.
        format!(
            "{}({})",
            send_node.method_name().unwrap_or_default(),
            arguments.join(", ")
        )
    } else {
        offense_source.to_owned()
    };
    if !source.starts_with('.') {
        source.insert(0, '.');
    }
    source.insert(0, '&');
    source
}

fn autocorrect(corrector: &mut Corrector, offense_range: Range, offense_source: &str, send_node: Node<'_>) {
    corrector.replace(
        offense_range,
        add_safe_navigation_operator(offense_source, send_node),
    );
    if require_parentheses(send_node) {
        corrector.wrap(send_node.source_range(), "(", ")");
    }
}

fn brackets(send_node: Node<'_>) -> bool {
    send_node.is_method("[]") || send_node.is_method("[]=")
}

fn require_parentheses(send_node: Node<'_>) -> bool {
    if operator_inside_collection_literal(send_node) {
        return true;
    }
    if !send_node.is_comparison_method() {
        return false;
    }
    let Some(node) = send_node.parent() else {
        return false;
    };

    (matches!(node.ty(), NodeType::And | NodeType::Or) && node.is_logical_operator())
        || (node.is_call() && node.is_comparison_method())
}

fn operator_inside_collection_literal(send_node: Node<'_>) -> bool {
    send_node
        .parent()
        .is_some_and(|p| matches!(p.ty(), NodeType::Array | NodeType::Pair))
        && send_node.loc().dot.is_none()
}
