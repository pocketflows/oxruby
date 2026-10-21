//! Port of `lib/rubocop/cop/style/redundant_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::comments_help::contains_comments;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Use double pipes `||` instead.";
const REDUNDANT_CONDITION: &str = "This condition is not needed.";
const ARGUMENT_WITH_OPERATOR_TYPES: &[NodeType] = &[
    NodeType::Splat,
    NodeType::BlockPass,
    NodeType::ForwardedRestarg,
    NodeType::ForwardedKwrestarg,
    NodeType::ForwardedArgs,
];

/// Ruby's structural `==` on optional nodes.
fn same(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
    }
}

/// `node.instance_of?(AST::Node)`: the type has no node class of its own in
/// rubocop-ast's `Builder::NODE_MAP`.
fn plain_node_class(node: Node<'_>) -> bool {
    use NodeType::*;
    !matches!(
        node.ty(),
        And | AndAsgn
            | Alias
            | Arg
            | Blockarg
            | ForwardArg
            | Kwarg
            | Kwoptarg
            | Kwrestarg
            | Optarg
            | Restarg
            | Shadowarg
            | Args
            | Array
            | Lvasgn
            | Ivasgn
            | Cvasgn
            | Gvasgn
            | Block
            | Numblock
            | Itblock
            | Break
            | CaseMatch
            | Casgn
            | Case
            | Class
            | Complex
            | Const
            | Def
            | Defined
            | Defs
            | Dstr
            | Ensure
            | For
            | ForwardArgs
            | ForwardedKwrestarg
            | Float
            | Hash
            | If
            | InPattern
            | Int
            | Index
            | Indexasgn
            | Irange
            | Erange
            | Kwargs
            | Kwbegin
            | Kwsplat
            | Lambda
            | Masgn
            | Mlhs
            | Module
            | Next
            | OpAsgn
            | OrAsgn
            | Or
            | Pair
            | Procarg0
            | Rational
            | Regexp
            | Rescue
            | Resbody
            | Return
            | Csend
            | Send
            | Str
            | Xstr
            | Sclass
            | Super
            | Zsuper
            | Sym
            | Until
            | UntilPost
            | Lvar
            | Ivar
            | Cvar
            | Gvar
            | When
            | While
            | WhilePost
            | Yield
    )
}

#[derive(Default)]
pub struct RedundantCondition;

impl Cop for RedundantCondition {
    fn name(&self) -> &'static str {
        "Style/RedundantCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_modifier_form() || node.is_elsif_conditional() || !offense(cx, node) {
            return;
        }
        let message = if redundant_condition(node) {
            REDUNDANT_CONDITION
        } else {
            MSG
        };
        let ps = cx.processed_source;
        cx.add_offense_with_correction(range_of_offense(node), message, |corrector| {
            if node.descendants().any(|d| contains_comments(ps, d)) {
                return;
            }
            if node.is_ternary() && !branches_have_method(node) {
                correct_ternary(corrector, node);
            } else if redundant_condition(node) {
                corrector.replace(node.source_range(), node.if_branch().unwrap().source());
            } else {
                corrector.replace(node.source_range(), make_ternary_form(node));
            }
        });
    }
}

fn range_of_offense(node: Node<'_>) -> Range {
    if !node.is_ternary() || branches_have_method(node) {
        return node.source_range();
    }
    range_between(
        node.loc().question.unwrap().begin_pos,
        node.loc().colon.unwrap().end_pos,
    )
}

fn offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let else_branch = node.child(2);
    if else_branch.is_some_and(|e| e.is(NodeType::If))
        || else_branch.is_some_and(|e| e.is_send() && e.is_method("[]="))
    {
        return false;
    }
    synonymous_condition_and_branch(cx, node)
        && !node.is_elsif()
        && (node.is_ternary()
            || !else_branch.is_some_and(plain_node_class)
            || else_branch.is_some_and(|e| e.is_single_line()))
}

fn redundant_condition(node: Node<'_>) -> bool {
    node.is_modifier_form() || node.else_branch().is_none()
}

fn synonymous_condition_and_branch(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let condition = node.child(0);
    let if_branch = node.child(1);
    if same(condition, if_branch) {
        return true;
    }
    if if_branch_is_true_type_and_else_is_not(cx, node) {
        return true;
    }
    if branches_have_assignment(node) && same(condition, if_branch.and_then(|b| b.expression())) {
        return true;
    }
    branches_have_method(node)
        && same(condition, if_branch.and_then(|b| b.first_argument()))
        && !if_branch.is_some_and(|b| b.is_send() && b.is_method("[]"))
}

fn if_branch_is_true_type_and_else_is_not(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !(node.is_ternary() || node.is_if()) {
        return false;
    }
    let Some(cond) = node.condition() else {
        return false;
    };
    if !cond.is_call()
        || !cond.is_predicate_method()
        || allowed_method(cx, cond.method_name().unwrap_or(""))
    {
        return false;
    }
    node.if_branch().is_some_and(|b| b.is(NodeType::True))
        && node
            .else_branch()
            .is_some_and(|e| !e.is(NodeType::True))
}

fn asgn_type(node: Node<'_>) -> bool {
    matches!(
        node.ty(),
        NodeType::Lvasgn | NodeType::Ivasgn | NodeType::Cvasgn | NodeType::Gvasgn | NodeType::Casgn
    )
}

fn branches_have_assignment(node: Node<'_>) -> bool {
    let (Some(if_branch), Some(else_branch)) = (node.child(1), node.child(2)) else {
        return false;
    };
    asgn_type(if_branch)
        && asgn_type(else_branch)
        && if_branch.name().is_some()
        && if_branch.name() == else_branch.name()
}

fn branches_have_method(node: Node<'_>) -> bool {
    let (Some(if_branch), Some(else_branch)) = (node.if_branch(), node.else_branch()) else {
        return false;
    };
    single_argument_method(if_branch)
        && single_argument_method(else_branch)
        && if_branch.method_name() == else_branch.method_name()
        && same(if_branch.receiver(), else_branch.receiver())
}

fn single_argument_method(node: Node<'_>) -> bool {
    if !node.is_send() || node.is_method("[]") || node.arguments().len() != 1 {
        return false;
    }
    !argument_with_operator(node.first_argument().unwrap())
}

fn argument_with_operator(argument: Node<'_>) -> bool {
    if ARGUMENT_WITH_OPERATOR_TYPES.contains(&argument.ty()) {
        return true;
    }
    if !argument.is(NodeType::Hash) {
        return false;
    }
    argument
        .child(0)
        .is_some_and(|n| matches!(n.ty(), NodeType::Kwsplat | NodeType::ForwardedKwrestarg))
}

fn wrap_arguments_with_parens(condition: Node<'_>) -> String {
    let method = condition
        .source_range()
        .begin()
        .join(condition.loc().selector.unwrap().end());
    let arguments = condition
        .first_argument()
        .unwrap()
        .source_range()
        .begin()
        .join(condition.source_range().end());
    format!(
        "{}({})",
        condition.range_source(method),
        condition.range_source(arguments)
    )
}

fn if_source(if_branch: Node<'_>, arithmetic_operation: bool) -> String {
    let parent = if_branch.parent().unwrap();
    if branches_have_method(parent) && if_branch.is_parenthesized() {
        if_branch
            .source()
            .strip_suffix(')')
            .unwrap_or(if_branch.source())
            .to_owned()
    } else if arithmetic_operation {
        format!(
            "{} {} ({}",
            if_branch.receiver().unwrap().source(),
            if_branch.method_name().unwrap_or(""),
            if_branch.first_argument().unwrap().source()
        )
    } else if if_branch.is(NodeType::True) {
        let condition = parent.condition().unwrap();
        if condition.arguments().is_empty() || condition.is_parenthesized() {
            condition.source().to_owned()
        } else {
            wrap_arguments_with_parens(condition)
        }
    } else {
        if_branch.source().to_owned()
    }
}

fn else_source(else_branch: Node<'_>, arithmetic_operation: bool) -> String {
    let parent = else_branch.parent().unwrap();
    if arithmetic_operation {
        format!("{})", else_branch.first_argument().unwrap().source())
    } else if branches_have_method(parent) {
        wrapped_source(else_branch.first_argument().unwrap())
    } else if require_parentheses(else_branch) {
        format!("({})", else_branch.source())
    } else if without_argument_parentheses_method(else_branch) {
        let args: Vec<&str> = else_branch.arguments().iter().map(|a| a.source()).collect();
        format!(
            "{}({})",
            else_branch.method_name().unwrap_or(""),
            args.join(", ")
        )
    } else if branches_have_assignment(parent) {
        wrapped_source(else_branch.expression().unwrap())
    } else {
        else_branch.source().to_owned()
    }
}

/// `else_source_if_has_method` / `else_source_if_has_assignment`.
fn wrapped_source(node: Node<'_>) -> String {
    if require_parentheses(node) {
        format!("({})", node.source())
    } else if node.is(NodeType::Hash) && !node.has_braces() {
        format!("{{ {} }}", node.source())
    } else {
        node.source().to_owned()
    }
}

fn make_ternary_form(node: Node<'_>) -> String {
    let if_branch = node.child(1).unwrap();
    let else_branch = node.child(2).unwrap();
    let arithmetic_operation = use_arithmetic_operation(if_branch);
    let mut ternary_form = format!(
        "{} || {}",
        if_source(if_branch, arithmetic_operation),
        else_source(else_branch, arithmetic_operation)
    );
    if branches_have_method(node) && if_branch.is_parenthesized() {
        ternary_form.push(')');
    }
    if node.parent().is_some_and(|p| p.is_send()) {
        format!("({ternary_form})")
    } else {
        ternary_form
    }
}

fn correct_ternary(corrector: &mut Corrector, node: Node<'_>) {
    corrector.replace(range_of_offense(node), "||");
    let else_branch = node.else_branch().unwrap();
    if else_branch.is_range() {
        corrector.wrap(else_branch.source_range(), "(", ")");
    }
}

fn require_parentheses(node: Node<'_>) -> bool {
    (node.is_basic_conditional() && node.is_modifier_form())
        || node.is_range()
        || node.is(NodeType::Rescue)
        || (matches!(node.ty(), NodeType::And | NodeType::Or) && node.is_semantic_operator())
}

fn use_arithmetic_operation(node: Node<'_>) -> bool {
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
    ) && node.is_arithmetic_operation()
}

fn without_argument_parentheses_method(node: Node<'_>) -> bool {
    node.is_send()
        && !node.arguments().is_empty()
        && !node.is_parenthesized()
        && !node.is_operator_method()
        && !node.is_assignment_method()
}
