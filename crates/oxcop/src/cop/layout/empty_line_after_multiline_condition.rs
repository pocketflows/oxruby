//! Port of `lib/rubocop/cop/layout/empty_line_after_multiline_condition.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "Use empty line after multiline condition.";

#[derive(Default)]
pub struct EmptyLineAfterMultilineCondition;

impl Cop for EmptyLineAfterMultilineCondition {
    fn name(&self) -> &'static str {
        "Layout/EmptyLineAfterMultilineCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[If, While, Until, WhilePost, UntilPost, Case, Rescue]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            If => {
                if node.is_ternary() {
                    return;
                }
                if node.is_modifier_form() {
                    if has_right_sibling(node) {
                        check_condition(cx, node.condition().unwrap());
                    }
                } else {
                    check_condition(cx, node.condition().unwrap());
                }
            }
            While | Until => check_condition(cx, node.condition().unwrap()),
            WhilePost | UntilPost => {
                if has_right_sibling(node) {
                    check_condition(cx, node.condition().unwrap());
                }
            }
            Case => on_case(cx, node),
            Rescue => on_rescue(cx, node),
            _ => {}
        }
    }
}

fn has_right_sibling(node: Node<'_>) -> bool {
    !matches!(node.right_sibling_value(), None | Some(Child::Nil))
}

fn on_case<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    for when_node in node.when_branches() {
        let conditions = when_node.conditions();
        let last_condition = *conditions.last().unwrap();
        if !multiline_when_condition(&conditions)
            || next_line_empty(cx.processed_source, last_condition.last_line())
        {
            continue;
        }
        add(cx, when_node, last_condition);
    }
}

fn on_rescue<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    for resbody in node.resbody_branches() {
        let rescued_exceptions = resbody.exceptions();
        if !multiline_rescue_exceptions(&rescued_exceptions)
            || next_line_empty(
                cx.processed_source,
                rescued_exceptions.last().unwrap().last_line(),
            )
        {
            continue;
        }
        add(cx, resbody, *rescued_exceptions.last().unwrap());
    }
}

fn check_condition<'a>(cx: &mut Investigation<'a, '_>, condition: Node<'a>) {
    if !condition.is_multiline() || next_line_empty(cx.processed_source, condition.last_line()) {
        return;
    }
    add(cx, condition, condition);
}

fn add<'a>(cx: &mut Investigation<'a, '_>, offense: Node<'a>, node: Node<'a>) {
    let ps = cx.processed_source;
    cx.add_offense_with_correction(offense.source_range(), MSG, |corrector| {
        let range = range_by_whole_lines(ps, node.source_range(), false);
        corrector.insert_after(range, "\n");
    });
}

fn next_line_empty(ps: &ProcessedSource<'_>, line: usize) -> bool {
    ps.lines()
        .get(line)
        .is_none_or(|l| l.chars().all(char::is_whitespace))
}

fn multiline_when_condition(conditions: &[Node<'_>]) -> bool {
    conditions.first().unwrap().first_line() != conditions.last().unwrap().last_line()
}

fn multiline_rescue_exceptions(exception_nodes: &[Node<'_>]) -> bool {
    if exception_nodes.len() <= 1 {
        return false;
    }
    exception_nodes.first().unwrap().first_line() != exception_nodes.last().unwrap().last_line()
}
