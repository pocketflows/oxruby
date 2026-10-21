//! Port of `lib/rubocop/cop/style/empty_case_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Do not use empty `case` condition, instead use an `if` expression.";
const NOT_SUPPORTED_PARENT_TYPES: &[NodeType] = &[
    NodeType::Return,
    NodeType::Break,
    NodeType::Next,
    NodeType::Send,
    NodeType::Csend,
    NodeType::Yield,
    NodeType::Super,
];

#[derive(Default)]
pub struct EmptyCaseCondition;

impl Cop for EmptyCaseCondition {
    fn name(&self) -> &'static str {
        "Style/EmptyCaseCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
        if case_node.condition().is_some()
            || case_node
                .parent()
                .is_some_and(|p| NOT_SUPPORTED_PARENT_TYPES.contains(&p.ty()))
        {
            return;
        }
        let mut branch_bodies: Vec<Node<'_>> = case_node
            .when_branches()
            .iter()
            .filter_map(|w| w.body())
            .collect();
        branch_bodies.extend(case_node.else_branch());
        if branch_bodies.iter().any(|body| {
            body.is(NodeType::Return) || body.descendants().any(|d| d.is(NodeType::Return))
        }) {
            return;
        }
        let ps = cx.processed_source;
        cx.add_offense_with_correction(case_node.loc().keyword.unwrap(), MSG, |corrector| {
            autocorrect(ps, corrector, case_node);
        });
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, case_node: Node<'_>) {
    let when_branches = case_node.when_branches();
    correct_case_when(ps, corrector, case_node, &when_branches);
    correct_when_conditions(corrector, &when_branches);
}

fn correct_case_when(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    case_node: Node<'_>,
    when_nodes: &[Node<'_>],
) {
    let case_range = case_node
        .loc()
        .keyword
        .unwrap()
        .join(when_nodes[0].loc().keyword.unwrap());
    corrector.replace(case_range, "if");
    keep_first_when_comment(ps, case_range, corrector);
    for when_node in &when_nodes[1..] {
        corrector.replace(when_node.loc().keyword.unwrap(), "elsif");
    }
}

fn correct_when_conditions(corrector: &mut Corrector, when_nodes: &[Node<'_>]) {
    for when_node in when_nodes {
        let conditions = when_node.conditions();
        replace_then_with_line_break(corrector, &conditions, *when_node);
        if conditions.len() <= 1 {
            continue;
        }
        let range = range_between(
            conditions[0].source_range().begin_pos,
            conditions[conditions.len() - 1].source_range().end_pos,
        );
        let joined: Vec<String> = conditions.iter().map(|c| parenthesize_condition(*c)).collect();
        corrector.replace(range, joined.join(" || "));
    }
}

fn parenthesize_condition(condition: Node<'_>) -> String {
    if condition.is_assignment()
        || matches!(condition.ty(), NodeType::If | NodeType::And | NodeType::Or)
        || condition.is_range()
    {
        format!("({})", condition.source())
    } else {
        condition.source().to_owned()
    }
}

fn keep_first_when_comment(ps: &ProcessedSource<'_>, case_range: Range, corrector: &mut Corrector) {
    let column = ps.column_of(case_range.begin_pos);
    let indent = " ".repeat(column);
    let first_line = ps.line_of(case_range.begin_pos);
    let last_line = ps.line_of(case_range.end_pos);
    let comments: String = if last_line > first_line {
        ps.comments_in_lines(first_line, last_line - 1)
            .iter()
            .map(|comment| format!("{indent}{}\n", comment.text))
            .collect()
    } else {
        String::new()
    };
    let line_beginning = case_range.with_begin(ps.line_start(first_line));
    corrector.insert_before(line_beginning, comments);
}

fn replace_then_with_line_break(corrector: &mut Corrector, conditions: &[Node<'_>], when_node: Node<'_>) {
    if when_node.parent().and_then(|p| p.parent()).is_none() || !when_node.has_then() {
        return;
    }
    let range = range_between(
        conditions[conditions.len() - 1].source_range().end_pos,
        when_node.loc().begin.unwrap().end_pos,
    );
    corrector.replace(range, "\n");
}
