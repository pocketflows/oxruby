//! Port of `lib/rubocop/cop/lint/void.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::ProcessedSource;

const SELF_MSG: &str = "`self` used in void context.";

const BINARY_OPERATORS: &[&str] = &[
    "*", "/", "%", "+", "-", "==", "===", "!=", "<", ">", "<=", ">=", "<=>",
];
const UNARY_OPERATORS: &[&str] = &["+@", "-@", "~", "!"];
const NONMUTATING_METHODS_WITH_BANG_VERSION: &[&str] = &[
    "capitalize",
    "chomp",
    "chop",
    "compact",
    "delete_prefix",
    "delete_suffix",
    "downcase",
    "encode",
    "flatten",
    "gsub",
    "lstrip",
    "merge",
    "next",
    "reject",
    "reverse",
    "rotate",
    "rstrip",
    "scrub",
    "select",
    "shuffle",
    "slice",
    "sort",
    "sort_by",
    "squeeze",
    "strip",
    "sub",
    "succ",
    "swapcase",
    "tr",
    "tr_s",
    "transform_values",
    "unicode_normalize",
    "uniq",
    "upcase",
];
const METHODS_REPLACEABLE_BY_EACH: &[&str] = &["collect", "map"];

fn is_operator(method: &str) -> bool {
    BINARY_OPERATORS.contains(&method) || UNARY_OPERATORS.contains(&method)
}

fn is_nonmutating_method(method: &str) -> bool {
    NONMUTATING_METHODS_WITH_BANG_VERSION.contains(&method)
        || METHODS_REPLACEABLE_BY_EACH.contains(&method)
}

#[derive(Default)]
pub struct Void;

impl Cop for Void {
    fn name(&self) -> &'static str {
        "Lint/Void"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Begin,
            NodeType::Kwbegin,
            NodeType::Ensure,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Begin | NodeType::Kwbegin => check_begin(cx, node),
            NodeType::Ensure => check_ensure(cx, node),
            _ => on_block(cx, node),
        }
    }
}

fn on_block(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(body) = node.body() else {
        return;
    };
    if body.is(NodeType::Begin) {
        return;
    }
    if !in_void_context(body) {
        return;
    }
    if node.is_method("each") {
        return;
    }

    check_void_op(cx, body, None);
    check_expression(cx, body);
}

fn check_begin(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let mut expressions: Vec<Node<'_>> = node.child_nodes().collect();
    let inside_each_block = node
        .ancestors()
        .find(|a| a.is_any_block())
        .is_some_and(|b| b.is_method("each"));
    if !in_void_context(node) || inside_each_block || setter_method(node) {
        expressions.pop();
    }
    for expr in expressions {
        check_void_op(cx, expr, Some(inside_each_block));
        check_expression(cx, expr);
    }
}

fn check_expression(cx: &mut Investigation<'_, '_>, expr: Node<'_>) {
    match expr.ty() {
        NodeType::If => check_if_expression(cx, expr),
        NodeType::Case | NodeType::CaseMatch => check_case_expression(cx, expr),
        _ => check_void_expression_nodes(cx, expr),
    }
}

fn check_void_expression_nodes(cx: &mut Investigation<'_, '_>, expr: Node<'_>) {
    check_literal(cx, expr);
    check_var(cx, expr);
    check_self(cx, expr);
    check_void_expression(cx, expr);
    if !cx.cop_config_bool("CheckForMethodsWithNoSideEffects", false) {
        return;
    }
    check_nonmutating(cx, expr);
}

fn check_if_expression(cx: &mut Investigation<'_, '_>, if_node: Node<'_>) {
    if let Some(body) = if_node.if_branch() {
        check_void_expression_nodes(cx, body);
    }
}

/// `check_case_expression` and `check_case_match_expression`.
fn check_case_expression(cx: &mut Investigation<'_, '_>, case_node: Node<'_>) {
    for when_node in case_node.when_branches() {
        if let Some(body) = when_node.body() {
            check_expression(cx, body);
        }
    }
    if let Some(else_branch) = case_node.else_branch() {
        check_expression(cx, else_branch);
    }
}

/// `inside_each_block` is the value the block given in Ruby yields; `None`
/// when no block is given.
fn check_void_op(cx: &mut Investigation<'_, '_>, node: Node<'_>, inside_each_block: Option<bool>) {
    let mut node = node;
    while node.is(NodeType::Begin) {
        match node.child(0) {
            Some(child) => node = child,
            None => return,
        }
    }
    let Some(method_name) = node.method_name() else {
        return;
    };
    if !(node.is_call() && is_operator(method_name)) {
        return;
    }
    if !UNARY_OPERATORS.contains(&method_name)
        && node.loc().dot.is_some()
        && node.arguments().is_empty()
    {
        return;
    }
    if inside_each_block == Some(true) {
        return;
    }

    let Some(selector) = node.loc().selector else {
        return;
    };
    let ps = cx.processed_source;
    let message = format!("Operator `{method_name}` used in void context.");
    cx.add_offense_with_correction(selector, &message, |corrector| {
        autocorrect_void_op(ps, corrector, node);
    });
}

fn check_var(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !(node.is_variable() || node.is(NodeType::Const)) {
        return;
    }
    let (offense_range, message) = if node.is(NodeType::Const) {
        let message = if node.is_special_keyword() {
            format!("Variable `{}` used in void context.", node.source())
        } else {
            format!("Constant `{}` used in void context.", node.source())
        };
        (node.source_range(), message)
    } else {
        let Some(name) = node.loc().name else {
            return;
        };
        (
            name,
            format!("Variable `{}` used in void context.", node.range_source(name)),
        )
    };
    let ps = cx.processed_source;
    cx.add_offense_with_correction(offense_range, &message, |corrector| {
        autocorrect_void_expression(ps, corrector, node);
    });
}

fn check_literal(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !entirely_literal(node)
        || node.is(NodeType::Xstr)
        || node.is_range()
        || node.is(NodeType::Nil)
    {
        return;
    }

    let ps = cx.processed_source;
    let message = format!("Literal `{}` used in void context.", node.source());
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        autocorrect_void_expression(ps, corrector, node);
    });
}

fn check_self(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !node.is(NodeType::SelfNode) {
        return;
    }

    let ps = cx.processed_source;
    cx.add_offense_with_correction(node.source_range(), SELF_MSG, |corrector| {
        autocorrect_void_expression(ps, corrector, node);
    });
}

fn check_void_expression(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !(node.is(NodeType::Defined) || node.is_lambda_or_proc()) {
        return;
    }

    let ps = cx.processed_source;
    let message = format!("`{}` used in void context.", node.source());
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        autocorrect_void_expression(ps, corrector, node);
    });
}

fn check_nonmutating(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !(node.is_call() || node.is_any_block()) {
        return;
    }
    let Some(method_name) = node.method_name() else {
        return;
    };
    if !is_nonmutating_method(method_name) {
        return;
    }

    let suggestion = if METHODS_REPLACEABLE_BY_EACH.contains(&method_name) {
        "each".to_owned()
    } else {
        format!("{method_name}!")
    };
    let message =
        format!("Method `#{method_name}` used in void context. Did you mean `#{suggestion}`?");
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        autocorrect_nonmutating_send(corrector, node, &suggestion);
    });
}

fn check_ensure(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(body) = node.ensure_branch() else {
        return;
    };
    if body.is(NodeType::Begin) {
        return;
    }

    check_expression(cx, body);
}

fn in_void_context(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.children().last() {
        Some(Child::Node(id)) if parent.ast().node(*id).same_as(node) => {}
        _ => return false,
    }

    parent.is_void_context()
}

fn setter_method(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is_any_def() && p.is_assignment_method())
}

fn autocorrect_void_op(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.arguments().is_empty() {
        if let Some(receiver) = node.receiver() {
            corrector.replace(node.source_range(), receiver.source());
        }
    } else {
        if let Some(dot) = node.loc().dot {
            corrector.remove(dot);
        }
        if let Some(selector) = node.loc().selector {
            corrector.replace(
                range_with_surrounding_space(
                    ps.buffer_source(),
                    selector,
                    SurroundingSpace {
                        side: Side::Both,
                        newlines: false,
                        ..SurroundingSpace::default()
                    },
                ),
                "\n",
            );
        }
    }
}

fn autocorrect_void_expression(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.is(NodeType::Const) && !node.is_special_keyword() {
        return;
    }
    if node.parent().is_some_and(|p| {
        matches!(
            p.ty(),
            NodeType::If | NodeType::Case | NodeType::When | NodeType::CaseMatch | NodeType::InPattern
        )
    }) {
        return;
    }
    if node
        .ancestors()
        .find(|a| a.is_any_def())
        .is_some_and(|d| d.is_assignment_method())
    {
        return;
    }

    corrector.remove(range_with_surrounding_space(
        ps.buffer_source(),
        node.source_range(),
        SurroundingSpace::side(Side::Left),
    ));
}

fn autocorrect_nonmutating_send(corrector: &mut Corrector, node: Node<'_>, suggestion: &str) {
    let send_node = if node.is_call() {
        Some(node)
    } else {
        node.send_node()
    };
    if let Some(selector) = send_node.and_then(|s| s.loc().selector) {
        corrector.replace(selector, suggestion);
    }
}

fn entirely_literal(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::Array => node.child_nodes().all(entirely_literal),
        NodeType::Hash => node.pairs().into_iter().all(|pair| {
            pair.key().is_some_and(entirely_literal) && pair.pair_value().is_some_and(entirely_literal)
        }),
        NodeType::Send | NodeType::Csend => {
            node.is_method("freeze") && node.receiver().is_some_and(entirely_literal)
        }
        _ => node.is_literal(),
    }
}
