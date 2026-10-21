//! Port of `lib/rubocop/cop/style/operator_method_call.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::parentheses_corrector;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Redundant dot detected.";

const INVALID_SYNTAX_ARG_TYPES: &[NodeType] = &[
    NodeType::Splat,
    NodeType::Kwsplat,
    NodeType::ForwardedArgs,
    NodeType::ForwardedRestarg,
    NodeType::ForwardedKwrestarg,
    NodeType::BlockPass,
];

#[derive(Default)]
pub struct OperatorMethodCall;

impl Cop for OperatorMethodCall {
    fn name(&self) -> &'static str {
        "Style/OperatorMethodCall"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "|", "^", "&", "<=>", "==", "===", "=~", ">", ">=", "<", "<=", "<<", ">>", "+", "-",
            "*", "/", "%", "**", "~", "!", "!=", "!~",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(dot) = node.loc().dot else {
            return;
        };
        if unary_method_no_operator(node) {
            return;
        }
        if node.receiver().is_some_and(|r| r.is(NodeType::Const)) || node.arguments().len() != 1 {
            return;
        }
        let Some(rhs) = node.first_argument() else {
            return;
        };
        if method_call_with_parenthesized_arg(rhs) || invalid_syntax_argument(rhs) {
            return;
        }
        cx.add_offense_with_correction(dot, MSG, |corrector| {
            wrap_in_parentheses_if_chained(corrector, node);
            corrector.replace(dot, " ");
            let selector = node.loc().selector.unwrap();
            if insert_space_after(node) {
                corrector.insert_after(selector, " ");
            }
        });
    }
}

/// `foo.~@` and `foo.!@` call `~` and `!` but are not written as operators.
fn unary_method_no_operator(node: Node<'_>) -> bool {
    let name = node.method_name().unwrap_or("");
    if !matches!(name, "+@" | "-@" | "~" | "!") {
        return false;
    }
    node.selector()
        .is_none_or(|s| node.range_source(s) != name)
}

fn method_call_with_parenthesized_arg(argument: Node<'_>) -> bool {
    let parent = argument.parent().unwrap();
    if !parent.parent().is_some_and(|gp| gp.is_send()) {
        return false;
    }
    !argument.children().is_empty() && !argument.children()[0].is_nil() && parent.is_parenthesized()
}

fn invalid_syntax_argument(argument: Node<'_>) -> bool {
    let ty = if argument.is(NodeType::Hash) {
        match argument.child(0) {
            Some(first) => first.ty(),
            None => return false,
        }
    } else {
        argument.ty()
    };
    INVALID_SYNTAX_ARG_TYPES.contains(&ty)
}

fn wrap_in_parentheses_if_chained(corrector: &mut Corrector, node: Node<'_>) {
    let Some(parent) = node.parent() else {
        return;
    };
    if !parent.is_call() || parent.first_argument() == Some(node) {
        return;
    }
    let operator = node.loc().selector.unwrap();
    parentheses_corrector::correct(corrector, node);
    corrector.insert_after(operator, " ");
    corrector.wrap(node.source_range(), "(", ")");
}

fn insert_space_after(node: Node<'_>) -> bool {
    let rhs = node.first_argument().unwrap();
    let selector = node.loc().selector.unwrap();
    if selector.end_pos == rhs.source_range().begin_pos {
        return true;
    }
    if node.parent().is_some_and(|p| p.is_call()) {
        return false;
    }
    let range = selector.end().join(rhs.source_range().begin());
    node.is_method("/") && node.range_source(range) == "("
}
