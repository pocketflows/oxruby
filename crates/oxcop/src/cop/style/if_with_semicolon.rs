//! Port of `lib/rubocop/cop/style/if_with_semicolon.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::on_normal_if_unless::normal_if_unless;
use crate::cop::{Cop, Corrector, Investigation};

#[derive(Default)]
pub struct IfWithSemicolon {
    ignored: IgnoredNodes,
}

impl Cop for IfWithSemicolon {
    fn name(&self) -> &'static str {
        "Style/IfWithSemicolon"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !normal_if_unless(node) {
            return;
        }
        if node.parent().is_some_and(|p| p.is(NodeType::If)) || self.ignored.part_of(node) {
            return;
        }
        let Some(beginning) = node.loc().begin else {
            return;
        };
        if node.range_source(beginning) != ";" {
            return;
        }
        let message = message(node);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            autocorrect(corrector, node);
        });
        self.ignored.ignore(node);
    }
}

fn message(node: Node<'_>) -> String {
    let template = if require_newline(node) {
        "use a newline instead"
    } else if node
        .else_branch()
        .is_some_and(|e| e.is_any(&[NodeType::If, NodeType::Begin]))
        || use_masgn_or_block_in_branches(node)
    {
        "use `if/else` instead"
    } else {
        "use a ternary operator instead"
    };
    format!(
        "Do not use `{} {};` - {template}.",
        node.keyword(),
        node.condition().unwrap().source()
    )
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    if require_newline(node) || use_masgn_or_block_in_branches(node) {
        corrector.replace(node.loc().begin.unwrap(), "\n");
    } else {
        corrector.replace(node.source_range(), replacement(node));
    }
}

fn branches<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    node.branches().into_iter().flatten().collect()
}

fn require_newline(node: Node<'_>) -> bool {
    branches(node).iter().any(|b| b.is(NodeType::Begin)) || use_return_with_argument(node)
}

fn use_masgn_or_block_in_branches(node: Node<'_>) -> bool {
    branches(node)
        .iter()
        .any(|b| b.is(NodeType::Masgn) || b.is_any_block())
}

fn use_return_with_argument(node: Node<'_>) -> bool {
    branches(node)
        .iter()
        .any(|b| b.is(NodeType::Return) && !b.arguments().is_empty())
}

fn replacement(node: Node<'_>) -> String {
    if node.else_branch().is_some_and(|e| e.is(NodeType::If)) {
        return correct_elsif(node);
    }
    let mut then_code = node
        .if_branch()
        .map_or_else(|| "nil".to_owned(), build_expression);
    let mut else_code = node
        .else_branch()
        .map_or_else(|| "nil".to_owned(), build_expression);
    if node.is_unless() {
        std::mem::swap(&mut then_code, &mut else_code);
    }
    format!("{} ? {then_code} : {else_code}", ternary_condition(node))
}

fn correct_elsif(node: Node<'_>) -> String {
    let else_branch = build_else_branch(node.else_branch().unwrap());
    format!(
        "if {}\n  {}\n{}\nend",
        node.condition().unwrap().source(),
        node.if_branch().map_or("", |b| b.source()),
        else_branch.strip_suffix('\n').unwrap_or(&else_branch)
    )
}

fn build_expression(expr: Node<'_>) -> String {
    if !require_argument_parentheses(expr) {
        return expr.source().to_owned();
    }
    let method = expr
        .source_range()
        .begin()
        .join(expr.loc().selector.unwrap().end());
    let arguments = expr
        .first_argument()
        .unwrap()
        .source_range()
        .begin()
        .join(expr.source_range().end());
    format!(
        "{}({})",
        expr.range_source(method),
        expr.range_source(arguments)
    )
}

/// An assignment used as the condition must be parenthesized, otherwise the
/// assignment would capture the whole ternary.
fn ternary_condition(node: Node<'_>) -> String {
    let condition = node.condition().unwrap();
    let assignment = if condition.is_call() {
        condition.is_setter_method()
    } else {
        condition.is_assignment()
    };
    if assignment {
        format!("({})", condition.source())
    } else {
        condition.source().to_owned()
    }
}

fn build_else_branch(second_condition: Node<'_>) -> String {
    let mut result = format!(
        "elsif {}\n  {}\n",
        second_condition.condition().unwrap().source(),
        second_condition.if_branch().map_or("", |b| b.source())
    );
    if let Some(else_branch) = second_condition.else_branch() {
        if else_branch.is(NodeType::If) {
            result += &build_else_branch(else_branch);
        } else {
            result += &format!("else\n  {}\n", else_branch.source());
        }
    }
    result
}

fn require_argument_parentheses(node: Node<'_>) -> bool {
    if !node.is_call() || node.is_arithmetic_operation() {
        return false;
    }
    !node.is_parenthesized()
        && !node.arguments().is_empty()
        && !node.is_method("[]")
        && !node.is_method("[]=")
}
