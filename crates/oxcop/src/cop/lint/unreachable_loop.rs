//! Port of `lib/rubocop/cop/lint/unreachable_loop.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Investigation};

const MSG: &str = "This loop will have at most one iteration.";
const CONTINUE_KEYWORDS: &[NodeType] = &[NodeType::Next, NodeType::Redo];

static BREAK_COMMAND: NodePattern = NodePattern::new(
    "{
       return break
       (send
        {nil? (const {nil? cbase} :Kernel)}
        {:raise :fail :throw :exit :exit! :abort}
        ...)
     }",
);

#[derive(Default)]
pub struct UnreachableLoop;

impl Cop for UnreachableLoop {
    fn name(&self) -> &'static str {
        "Lint/UnreachableLoop"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::While,
            NodeType::Until,
            NodeType::WhilePost,
            NodeType::UntilPost,
            NodeType::For,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() && !loop_method(cx, node) {
            return;
        }
        check(cx, node);
    }
}

fn loop_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !node.is_any_block() {
        return false;
    }
    let Some(send_node) = node.send_node() else {
        return false;
    };
    let loopable = send_node.is_enumerable_method()
        || send_node.is_enumerator_method()
        || send_node.is_method("loop");
    loopable && !matches_allowed_pattern(cx, send_node.source())
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let statements = statements(node);
    let Some(break_statement) = statements
        .into_iter()
        .find(|statement| break_statement(cx, *statement))
    else {
        return;
    };

    if !(preceded_by_continue_statement(cx, break_statement)
        || conditional_continue_keyword(break_statement))
    {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn statements(node: Node<'_>) -> Vec<Node<'_>> {
    match node.body() {
        None => Vec::new(),
        Some(body) if body.is(NodeType::Begin) => body.child_nodes().collect(),
        Some(body) => vec![body],
    }
}

fn break_statement(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if BREAK_COMMAND.is_match(node) {
        return true;
    }

    match node.ty() {
        NodeType::Begin | NodeType::Kwbegin => {
            let found = node
                .child_nodes()
                .find(|statement| break_statement(cx, *statement));
            found.is_some_and(|b| !preceded_by_continue_statement(cx, b))
        }
        NodeType::If => check_if(cx, node),
        NodeType::Case | NodeType::CaseMatch => check_case(cx, node),
        _ => false,
    }
}

fn check_if(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let (Some(if_branch), Some(else_branch)) = (node.if_branch(), node.else_branch()) else {
        return false;
    };
    break_statement(cx, if_branch) && break_statement(cx, else_branch)
}

fn check_case(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(else_branch) = node.else_branch() else {
        return false;
    };
    if !break_statement(cx, else_branch) {
        return false;
    }

    node.when_branches()
        .into_iter()
        .all(|branch| branch.body().is_some_and(|body| break_statement(cx, body)))
}

fn preceded_by_continue_statement(cx: &Investigation<'_, '_>, break_statement: Node<'_>) -> bool {
    break_statement.left_siblings().into_iter().any(|sibling| {
        if sibling.is_loop_keyword() || loop_method(cx, sibling) {
            return false;
        }
        sibling
            .descendants()
            .any(|d| CONTINUE_KEYWORDS.contains(&d.ty()))
    })
}

fn conditional_continue_keyword(break_statement: Node<'_>) -> bool {
    let Some(or_node) = break_statement
        .descendants()
        .filter(|d| d.is(NodeType::Or))
        .last()
    else {
        return false;
    };
    or_node
        .rhs()
        .is_some_and(|rhs| CONTINUE_KEYWORDS.contains(&rhs.ty()))
}
