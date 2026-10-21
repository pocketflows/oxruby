//! Port of `lib/rubocop/cop/style/sole_nested_conditional.rb`.

use crate::ast::{Child, Node, NodeId, NodeType};
use crate::cop::mixin::range_help::{
    range_between, range_by_whole_lines, range_with_surrounding_space, SurroundingSpace,
};
use crate::cop::mixin::reparsed_equivalence::{correction_parses, ReparsedEquivalence};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct SoleNestedConditional {
    /// `ignore_node`/`ignored_node?` (identity, within one investigation).
    ignored: Vec<NodeId>,
}

impl Cop for SoleNestedConditional {
    fn name(&self) -> &'static str {
        "Style/SoleNestedConditional"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/NegatedIf", "Style/NegatedUnless"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !offending_conditional(cx, node) {
            return;
        }
        let if_branch = node.if_branch().unwrap();
        if !correction_parses(self, cx, node) {
            return;
        }
        let message = format!(
            "Consider merging nested conditions into outer `{}` conditions.",
            node.keyword()
        );
        let ignored = self.ignored.contains(&node.id());
        let ps = cx.processed_source;
        let mut corrected = false;
        cx.add_offense_with_correction(if_branch.loc().keyword.unwrap(), &message, |corrector| {
            if ignored {
                return;
            }
            autocorrect(ps, corrector, node, if_branch);
            corrected = true;
        });
        if corrected {
            self.ignored.push(if_branch.id());
        }
    }
}

impl ReparsedEquivalence<Node<'_>> for SoleNestedConditional {
    fn apply_reparse_correction(
        &self,
        cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        node: Node<'_>,
    ) {
        autocorrect(cx.processed_source, corrector, node, node.if_branch().unwrap());
    }

    fn item_range(&self, node: Node<'_>) -> Range {
        node.source_range()
    }
}

fn offending_conditional(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if node.is_ternary() || node.has_else() || node.is_elsif() {
        return false;
    }
    let if_branch = node.if_branch();
    if use_variable_assignment_in_condition(node.condition().unwrap(), if_branch) {
        return false;
    }
    offending_branch(cx, node, if_branch)
}

fn use_variable_assignment_in_condition(condition: Node<'_>, if_branch: Option<Node<'_>>) -> bool {
    let Some(if_branch) = if_branch.filter(|b| b.is(NodeType::If)) else {
        return false;
    };
    let assigned = assigned_variables(condition);
    let source = if_branch.condition().unwrap().source();
    assigned.iter().any(|v| v == source)
}

/// `assignment?`: assignment node types, or a setter call.
fn assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

/// `node.children.first.to_s` of each assignment in the condition.
fn assigned_variables(condition: Node<'_>) -> Vec<String> {
    condition
        .each_node()
        .filter(|n| assignment(*n))
        .map(|n| match n.children().first() {
            Some(Child::Sym(name)) => name.clone(),
            Some(Child::Nil) | None => String::new(),
            // A node's `to_s` is its s-expression, which never equals source.
            Some(_) => "\0".to_owned(),
        })
        .collect()
}

fn offending_branch(cx: &Investigation<'_, '_>, node: Node<'_>, branch: Option<Node<'_>>) -> bool {
    let Some(branch) = branch else {
        return false;
    };
    branch.is(NodeType::If)
        && !branch.has_else()
        && !branch.is_ternary()
        && !((node.is_modifier_form() || branch.is_modifier_form())
            && cx.cop_config_bool("AllowModifier", false))
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>, if_branch: Node<'_>) {
    if node.is_modifier_form() {
        autocorrect_outer_condition_modify_form(ps, corrector, node, if_branch);
    } else {
        autocorrect_outer_condition_basic(ps, corrector, node, if_branch);
    }
}

fn autocorrect_outer_condition_basic(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    if_branch: Node<'_>,
) {
    correct_node(ps, corrector, node);
    if if_branch.is_modifier_form() {
        correct_for_guard_condition_style(ps, corrector, node, if_branch);
    } else {
        correct_for_basic_condition_style(ps, corrector, node, if_branch);
        correct_for_comment(ps, corrector, node, if_branch);
    }
}

fn correct_node(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.is_unless() {
        corrector.replace(node.loc().keyword.unwrap(), "if");
    }
    corrector.replace(
        node.condition().unwrap().source_range(),
        chainable_condition(ps, node),
    );
}

fn correct_for_guard_condition_style(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    if_branch: Node<'_>,
) {
    corrector.insert_after(
        node.condition().unwrap().source_range(),
        format!(" && {}", chainable_condition(ps, if_branch)),
    );
    let range = range_between(
        if_branch.loc().keyword.unwrap().begin_pos,
        if_branch.condition().unwrap().source_range().end_pos,
    );
    corrector.remove(range_with_surrounding_space(
        ps.buffer_source(),
        range,
        SurroundingSpace {
            newlines: false,
            ..SurroundingSpace::default()
        },
    ));
}

fn correct_for_basic_condition_style(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    if_branch: Node<'_>,
) {
    let range = range_between(
        node.condition().unwrap().source_range().end_pos,
        if_branch.condition().unwrap().source_range().begin_pos,
    );
    corrector.replace(range, " && ");
    corrector.replace(
        if_branch.condition().unwrap().source_range(),
        chainable_condition(ps, if_branch),
    );
    let node_end = node.loc().end.unwrap();
    let end_range = match if_branch.loc().end {
        Some(branch_end) if same_line(ps, node_end, branch_end) => node_end,
        _ => range_by_whole_lines(ps, node_end, true),
    };
    corrector.remove(end_range);
}

fn autocorrect_outer_condition_modify_form(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    if_branch: Node<'_>,
) {
    correct_node(ps, corrector, if_branch);
    corrector.insert_before(
        if_branch.condition().unwrap().source_range(),
        format!("{} && ", chainable_condition(ps, node)),
    );
    let range = range_between(
        node.loc().keyword.unwrap().begin_pos,
        node.condition().unwrap().source_range().end_pos,
    );
    corrector.remove(range_with_surrounding_space(
        ps.buffer_source(),
        range,
        SurroundingSpace {
            newlines: false,
            ..SurroundingSpace::default()
        },
    ));
}

fn correct_for_comment(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    if_branch: Node<'_>,
) {
    let condition_line = if_branch.condition().unwrap().first_line();
    let comments: Vec<&str> = ps
        .node_comments(if_branch)
        .into_iter()
        .filter(|c| ps.line_of(c.range.begin_pos) < condition_line)
        .map(|c| c.text)
        .collect();
    if comments.is_empty() {
        return;
    }
    corrector.insert_before(
        node.loc().keyword.unwrap(),
        format!("{}\n", comments.join("\n")),
    );
}

fn chainable_condition(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    let condition = node.condition().unwrap();
    let wrapped = add_parentheses_if_needed(ps, condition);
    if node.is_if() {
        return wrapped;
    }
    if condition.is(NodeType::And) {
        format!("!({wrapped})")
    } else {
        format!("!{wrapped}")
    }
}

fn add_parentheses_if_needed(ps: &ProcessedSource<'_>, condition: Node<'_>) -> String {
    let node_to_check = if condition.is_any_block() {
        condition.send_node().unwrap()
    } else {
        condition
    };
    if !add_parentheses(node_to_check) {
        return condition.source().to_owned();
    }
    if parenthesize_method(condition) {
        parenthesized_method_arguments(condition)
    } else if condition.is(NodeType::And) {
        parenthesized_and(ps, condition)
    } else {
        format!("({})", condition.source())
    }
}

fn parenthesize_method(node: Node<'_>) -> bool {
    node.is_call()
        && node.has_arguments()
        && !node.is_parenthesized()
        && !node.is_comparison_method()
        && !node.is_operator_method()
}

fn add_parentheses(node: Node<'_>) -> bool {
    if assignment(node) || node.is(NodeType::Or) {
        return true;
    }
    if node.is(NodeType::And) && node.descendants().any(assignment) {
        return true;
    }
    if !node.is_call() {
        return false;
    }
    (node.has_arguments() && !node.is_parenthesized()) || node.is_prefix_not()
}

fn parenthesized_method_arguments(node: Node<'_>) -> String {
    let method_call = node
        .source_range()
        .begin()
        .join(node.loc().selector.unwrap().end());
    let arguments = node
        .first_argument()
        .unwrap()
        .source_range()
        .begin()
        .join(node.source_range().end());
    format!(
        "{}({})",
        node.range_source(method_call),
        node.range_source(arguments)
    )
}

fn parenthesized_and(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    let lhs = node.lhs().unwrap().source();
    let rhs = parenthesized_and_clause(ps, node.rhs().unwrap());
    let operator = range_with_surrounding_space(
        ps.buffer_source(),
        node.loc().operator.unwrap(),
        SurroundingSpace {
            whitespace: true,
            ..SurroundingSpace::default()
        },
    );
    format!("{lhs}{}{rhs}", ps.slice(operator))
}

fn parenthesized_and_clause(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    if node.is(NodeType::And) {
        parenthesized_and(ps, node)
    } else if assignment(node) {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}
