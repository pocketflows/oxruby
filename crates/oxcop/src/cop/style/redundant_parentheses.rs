//! Port of `lib/rubocop/cop/style/redundant_parentheses.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Child, Node, NodeId, NodeType};
use crate::cop::correctors::parentheses_corrector;
use crate::cop::mixin::parentheses::parens_required;
use crate::cop::mixin::reparsed_equivalence::{
    Oversized, ReparsedEquivalence, verified_by_reparse,
};
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const ALLOWED_NODE_TYPES: &[NodeType] =
    &[NodeType::Or, NodeType::Send, NodeType::Splat, NodeType::Kwsplat];

#[derive(Default)]
pub struct RedundantParentheses {
    /// `@pending_offenses`, in insertion order.
    pending_offenses: Vec<(NodeId, String)>,
}

impl Cop for RedundantParentheses {
    fn name(&self) -> &'static str {
        "Style/RedundantParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Begin]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.pending_offenses.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !parentheses(node) || parens_allowed(cx, node) || ignore_syntax(node) {
            return;
        }
        self.check(cx, node);
    }

    fn on_investigation_end(&mut self, cx: &mut Investigation<'_, '_>) {
        if self.pending_offenses.is_empty() {
            return;
        }
        let tree = cx.processed_source.tree();
        let items: Vec<Node<'_>> = self
            .pending_offenses
            .iter()
            .map(|(id, _)| tree.node(*id))
            .collect();
        let verified = verified_by_reparse(&*self, cx, &items, Oversized::Report);
        for node in verified {
            let message = self
                .pending_offenses
                .iter()
                .find(|(id, _)| *id == node.id())
                .map(|(_, m)| m.clone())
                .unwrap_or_default();
            cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
                parentheses_corrector::correct(corrector, node);
            });
        }
    }
}

impl<'a> ReparsedEquivalence<Node<'a>> for RedundantParentheses {
    fn apply_reparse_correction(
        &self,
        _cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        item: Node<'a>,
    ) {
        parentheses_corrector::correct(corrector, item);
    }

    fn item_range(&self, item: Node<'a>) -> Range {
        item.source_range()
    }

    fn reparsed_equal(&self, original: Node<'_>, rewritten: Node<'_>) -> bool {
        normalize_reparsed_ast(original) == normalize_reparsed_ast(rewritten)
    }
}

impl RedundantParentheses {
    fn offense(&mut self, node: Node<'_>, msg: &str) {
        let message = format!("Don't use parentheses around {msg}.");
        match self.pending_offenses.iter_mut().find(|(id, _)| *id == node.id()) {
            Some(entry) => entry.1 = message,
            None => self.pending_offenses.push((node.id(), message)),
        }
    }

    fn check(&mut self, cx: &Investigation<'_, '_>, begin_node: Node<'_>) {
        let Some(node) = begin_node.child(0) else {
            return;
        };
        if let Some(message) = find_offense_message(cx, begin_node, node) {
            if message == "block body" {
                return self.offense(begin_node, message);
            }
            let mut target = begin_node;
            if node.is_range() && !argument_of_parenthesized_method_call(begin_node, node) {
                target = begin_node.parent().unwrap();
            }
            return self.offense(target, message);
        }
        if call_node(node) {
            self.check_send(begin_node, node);
        }
    }

    fn check_send(&mut self, begin_node: Node<'_>, node: Node<'_>) {
        let node = if node.is_any_block() {
            node.send_node().unwrap()
        } else {
            node
        };
        if node.is_unary_operation() {
            return self.check_unary(begin_node, node);
        }
        if method_call_with_redundant_parentheses(begin_node, node) {
            self.offense(begin_node, "a method call");
        }
    }

    fn check_unary(&mut self, begin_node: Node<'_>, node: Node<'_>) {
        if begin_node.is_chained() {
            return;
        }
        let mut node = node;
        while suspect_unary(node) {
            node = node.child(0).unwrap();
        }
        if method_call_with_redundant_parentheses(begin_node, node) {
            self.offense(begin_node, "a unary operation");
        }
    }
}

/// A tree after `normalize_reparsed_ast`.
#[derive(PartialEq, Debug)]
enum Norm {
    Node(NodeType, Vec<Norm>),
    Leaf(Child),
}

fn normalize_reparsed_ast(node: Node<'_>) -> Norm {
    let mut children: Vec<Norm> = node
        .children()
        .iter()
        .map(|child| match child {
            Child::Node(id) => normalize_reparsed_ast(node.ast().node(*id)),
            other => Norm::Leaf(other.clone()),
        })
        .collect();
    if matches!(node.ty(), NodeType::Begin | NodeType::Kwbegin) {
        children = children
            .into_iter()
            .flat_map(|child| match child {
                Norm::Node(NodeType::Begin, grandchildren) => grandchildren,
                other => vec![other],
            })
            .collect();
    }
    if node.is(NodeType::Begin) && children.len() == 1 && matches!(children[0], Norm::Node(..)) {
        return children.pop().unwrap();
    }
    rotate_same_operator(Norm::Node(node.ty(), children))
}

fn rotate_same_operator(node: Norm) -> Norm {
    let Norm::Node(ty, mut children) = node else {
        return node;
    };
    if !matches!(ty, NodeType::And | NodeType::Or) || children.len() != 2 {
        return Norm::Node(ty, children);
    }
    if !matches!(&children[1], Norm::Node(t, c) if *t == ty && c.len() == 2) {
        return Norm::Node(ty, children);
    }
    let Some(Norm::Node(_, mut right)) = children.pop() else {
        unreachable!()
    };
    let lhs = children.pop().unwrap();
    let right_rhs = right.pop().unwrap();
    let right_lhs = right.pop().unwrap();
    let rotated_left = rotate_same_operator(Norm::Node(ty, vec![lhs, right_lhs]));
    rotate_same_operator(Norm::Node(ty, vec![rotated_left, right_rhs]))
}

fn parens_allowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.children().is_empty()
        || rescue(node)
        || in_pattern_matching_in_method_argument(cx, node)
        || allowed_pin_operator(node)
        || allowed_expression(cx, node)
}

/// `{^resbody ^^resbody}`
fn rescue(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        p.is(NodeType::Resbody) || p.parent().is_some_and(|gp| gp.is(NodeType::Resbody))
    })
}

/// `^(pin (begin !{lvar ivar cvar gvar}))`
fn allowed_pin_operator(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is(NodeType::Pin))
        && node.children().len() == 1
        && !node.child(0).is_some_and(|c| c.is_variable())
}

fn ignore_syntax(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.ty(),
        NodeType::WhilePost | NodeType::UntilPost | NodeType::MatchWithLvasgn
    ) || like_method_argument_parentheses(parent)
        || multiline_control_flow_statements(node)
}

fn allowed_expression(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    (node.parent().is_some_and(|p| p.is_keyword()) && parens_required(node))
        || allowed_multiple_expression(node)
        || allowed_ternary(cx, node)
        || node.parent().is_some_and(|p| p.is_range())
}

fn allowed_multiple_expression(node: Node<'_>) -> bool {
    if node.children().len() == 1 {
        return false;
    }
    let Some(ancestor) = node.parent() else {
        return false;
    };
    !(ancestor.is(NodeType::Begin) || ancestor.is_any_def() || ancestor.is_any_block())
}

fn allowed_ternary(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(parent) = node.parent().filter(|p| p.is(NodeType::If)) else {
        return false;
    };
    parent.is_ternary() && ternary_parentheses_required(cx)
}

fn ternary_parentheses_required(cx: &Investigation<'_, '_>) -> bool {
    let config = cx.config.for_cop("Style/TernaryParentheses");
    let enabled = !matches!(
        config.get("Enabled"),
        None | Some(Value::Bool(false)) | Some(Value::Null)
    );
    enabled
        && matches!(
            config.get("EnforcedStyle").and_then(Value::as_str),
            Some("require_parentheses" | "require_parentheses_when_complex")
        )
}

fn like_method_argument_parentheses(node: Node<'_>) -> bool {
    if !matches!(node.ty(), NodeType::Send | NodeType::Super | NodeType::Yield) {
        return false;
    }
    let args = node.arguments();
    args.len() == 1
        && !node.is_parenthesized()
        && !node.is_operator_method()
        && args[0].is(NodeType::Begin)
}

fn multiline_control_flow_statements(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.is_single_line() {
        return false;
    }
    matches!(
        parent.ty(),
        NodeType::Return | NodeType::Next | NodeType::Break
    )
}

fn in_pattern_matching_in_method_argument(cx: &Investigation<'_, '_>, begin_node: Node<'_>) -> bool {
    if !begin_node.parent().is_some_and(|p| p.is_call()) {
        return false;
    }
    let Some(node) = begin_node.child(0) else {
        return false;
    };
    if cx.target_ruby_version() <= 2.7 {
        node.is(NodeType::MatchPattern)
    } else {
        node.is(NodeType::MatchPatternP)
    }
}

fn find_offense_message(
    cx: &Investigation<'_, '_>,
    begin_node: Node<'_>,
    node: Node<'_>,
) -> Option<&'static str> {
    let parent = begin_node.parent();
    if keyword_with_redundant_parentheses(node) {
        return Some("a keyword");
    }
    if node.is_literal() && disallowed_literal(begin_node, node) {
        return Some("a literal");
    }
    if node.is_variable() {
        return Some("a variable");
    }
    if node.is(NodeType::Const) {
        return Some("a constant");
    }
    if parent.is_some_and(|p| p.is_any_block()) || body_range(begin_node, node) {
        return Some("block body");
    }
    if node.is_assignment() && parent.is_none_or(|p| p.is(NodeType::Begin)) {
        return Some("an assignment");
    }
    if node.is_lambda_or_proc()
        && node.is_any_block()
        && (node.has_braces() || node.send_node().is_some_and(|s| s.is_lambda_literal()))
    {
        return Some("an expression");
    }
    if disallowed_one_line_pattern_matching(begin_node, node) {
        return Some("a one-line pattern matching");
    }
    if interpolation(begin_node) {
        return Some("an interpolated expression");
    }
    if argument_of_parenthesized_method_call(begin_node, node) && !keyword_logical_operator(node) {
        return Some("a method argument");
    }
    if oneline_rescue_parentheses_required(begin_node, node) {
        return Some("a one-line rescue");
    }
    if begin_node.is_chained() {
        return None;
    }
    if node.is_operator_keyword() {
        if node.is_semantic_operator() && parent.is_some() {
            return None;
        }
        if node.is_multiline() && allow_in_multiline_conditions(cx) {
            return None;
        }
        if parent.is_some_and(|p| ALLOWED_NODE_TYPES.contains(&p.ty())) {
            return None;
        }
        if !node.is(NodeType::And) && parent.is_some_and(|p| p.is(NodeType::And)) {
            return None;
        }
        if parent.is_some_and(|p| p.is(NodeType::If) && p.is_ternary()) {
            return None;
        }
        Some("a logical expression")
    } else if comparison_method(node) {
        parent.is_none().then_some("a comparison expression")
    } else {
        None
    }
}

/// `node.respond_to?(:comparison_method?) && node.comparison_method?`.
fn comparison_method(node: Node<'_>) -> bool {
    matches!(
        node.ty(),
        NodeType::Send
            | NodeType::Csend
            | NodeType::Super
            | NodeType::Zsuper
            | NodeType::Yield
            | NodeType::Index
            | NodeType::Indexasgn
            | NodeType::Lambda
            | NodeType::Defined
            | NodeType::Def
            | NodeType::Defs
            | NodeType::Block
            | NodeType::Numblock
            | NodeType::Itblock
    ) && node.is_comparison_method()
}

/// `[^begin ^^dstr]`
fn interpolation(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        p.is(NodeType::Begin) && p.parent().is_some_and(|gp| gp.is(NodeType::Dstr))
    })
}

fn argument_of_parenthesized_method_call(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if node.is_basic_conditional()
        || node.is(NodeType::Rescue)
        || method_call_parentheses_required(node)
    {
        return false;
    }
    let Some(parent) = begin_node.parent() else {
        return false;
    };
    parent.is_call() && parent.is_parenthesized() && parent.receiver() != Some(begin_node)
}

fn oneline_rescue_parentheses_required(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if !node.is(NodeType::Rescue) {
        return false;
    }
    let Some(parent) = begin_node.parent() else {
        return false;
    };
    if parent.is(NodeType::If) && parent.is_ternary() {
        return false;
    }
    if parent.is_conditional() && parent.condition() == Some(begin_node) {
        return false;
    }
    !(parent.is_call() || matches!(parent.ty(), NodeType::Array | NodeType::Pair))
}

fn method_call_parentheses_required(node: Node<'_>) -> bool {
    if !node.is_call() {
        return false;
    }
    (node.receiver().is_none() || node.loc().dot.is_some()) && node.has_arguments()
}

/// `!!config.for_enabled_cop('Style/ParenthesesAroundCondition')['AllowInMultilineConditions']`
fn allow_in_multiline_conditions(cx: &Investigation<'_, '_>) -> bool {
    let name = "Style/ParenthesesAroundCondition";
    cx.config.cop_enabled(name)
        && !matches!(
            cx.config.for_cop(name).get("AllowInMultilineConditions"),
            None | Some(Value::Bool(false)) | Some(Value::Null)
        )
}

fn call_node(node: Node<'_>) -> bool {
    node.is_call() || (node.is_any_block() && node.has_braces() && !node.is_lambda_or_proc())
}

fn suspect_unary(node: Node<'_>) -> bool {
    node.is_send() && node.is_unary_operation() && !node.is_prefix_not()
}

fn disallowed_literal(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if !node.is_range() {
        return true;
    }
    let Some(parent) = begin_node.parent() else {
        return false;
    };
    parent.is(NodeType::Begin) && parent.children().len() == 1
}

fn body_range(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if begin_node.is_chained() || !node.is_range() {
        return false;
    }
    let Some(parent) = begin_node.parent().filter(|p| p.is(NodeType::Begin)) else {
        return false;
    };
    (node.child(0).is_none() && parent.child_nodes().next() == Some(begin_node))
        || (node.child(1).is_none() && parent.child_nodes().last() == Some(begin_node))
}

fn keyword_logical_operator(node: Node<'_>) -> bool {
    node.is_operator_keyword() && node.is_semantic_operator()
}

fn disallowed_one_line_pattern_matching(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if let Some(parent) = begin_node.parent() {
        if parent.is_any_def() && parent.is_endless() {
            return false;
        }
        if parent.is_assignment() {
            return false;
        }
    }
    node.is_any_match_pattern() && !node.ancestors().any(|a| a.is_operator_keyword())
}

fn keyword_with_redundant_parentheses(node: Node<'_>) -> bool {
    if !node.is_keyword() {
        return false;
    }
    if node.is_special_keyword() {
        return true;
    }
    let args = node.children();
    if args.len() == 1
        && let Some(first) = node.child(0).filter(|f| f.is(NodeType::Begin))
    {
        parentheses(first)
    } else {
        args.is_empty() || parentheses(node)
    }
}

fn method_call_with_redundant_parentheses(begin_node: Node<'_>, node: Node<'_>) -> bool {
    if !(node.is_call()
        || matches!(
            node.ty(),
            NodeType::Super | NodeType::Zsuper | NodeType::Yield | NodeType::Defined
        ))
    {
        return false;
    }
    if node.is_send() && node.is_prefix_not() {
        return false;
    }
    if singular_parenthesized_parent(begin_node) {
        return true;
    }
    node.arguments().is_empty() || parentheses(node) || square_brackets(node)
}

/// ``(send `{(send _recv _msg) str array hash const #variable?} :[] ...)``
fn square_brackets(node: Node<'_>) -> bool {
    node.is_send()
        && node.is_method("[]")
        && node.receiver().is_some_and(|receiver| {
            receiver.each_node().any(|n| {
                (n.is_send() && n.children().len() == 2)
                    || matches!(
                        n.ty(),
                        NodeType::Str | NodeType::Array | NodeType::Hash | NodeType::Const
                    )
                    || n.is_variable()
            })
        })
}

fn singular_parenthesized_parent(begin_node: Node<'_>) -> bool {
    let Some(parent) = begin_node.parent() else {
        return true;
    };
    if matches!(parent.ty(), NodeType::Splat | NodeType::Kwsplat) {
        return false;
    }
    parent.children().len() == 1
}
