//! Port of `lib/rubocop/cop/style/case_like_if.rb`.
//!
//! `regexp_with_named_captures?` uses `RegexpNode#each_capture(named: true)`
//! (the `regexp_parser` gem upstream); here a named group is recognized in
//! the regexp's literal parts directly.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::min_branches_count::min_branches_count;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::indent;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const MSG: &str = "Convert `if-elsif` to `case-when`.";

#[derive(Default)]
pub struct CaseLikeIf;

impl Cop for CaseLikeIf {
    fn name(&self) -> &'static str {
        "Style/CaseLikeIf"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !should_check(cx, node) {
            return;
        }
        let Some(target) = node.condition().and_then(find_target) else {
            return;
        };
        for branch_condition in branch_conditions(node) {
            if regexp_with_working_captures(branch_condition) {
                return;
            }
            let mut conditions = Vec::new();
            if !collect_conditions(branch_condition, target, &mut conditions) {
                return;
            }
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.insert_before(
                node.source_range(),
                format!("case {}\n{}", target.source(), indent(node, 0)),
            );
            for branch_condition in branch_conditions(node) {
                let mut conditions = Vec::new();
                collect_conditions(branch_condition, target, &mut conditions);
                let sources: Vec<&str> = conditions.iter().map(|c| c.source()).collect();
                corrector.replace(
                    correction_range(branch_condition),
                    format!("when {}", sources.join(", ")),
                );
            }
        });
    }
}

fn should_check(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !node.is_unless()
        && !node.is_elsif()
        && !node.is_modifier_form()
        && !node.is_ternary()
        && node.is_elsif_conditional()
        && min_branches_count(cx, node)
}

fn find_target(node: Node<'_>) -> Option<Node<'_>> {
    match node.ty() {
        NodeType::Begin => find_target(node.child(0)?),
        NodeType::Or => find_target(node.lhs()?),
        NodeType::MatchWithLvasgn => {
            let (lhs, rhs) = (node.child(0)?, node.child(1)?);
            if lhs.is(NodeType::Regexp) {
                Some(rhs)
            } else if rhs.is(NodeType::Regexp) {
                Some(lhs)
            } else {
                None
            }
        }
        NodeType::Send => find_target_in_send_node(node),
        _ => None,
    }
}

fn find_target_in_send_node(node: Node<'_>) -> Option<Node<'_>> {
    match node.method_name()? {
        "is_a?" => node.receiver(),
        "==" | "eql?" | "equal?" => {
            let argument = node.first_argument()?;
            let receiver = node.receiver()?;
            if argument.is_literal() || const_reference(argument) {
                Some(receiver)
            } else if receiver.is_literal() || const_reference(receiver) {
                Some(argument)
            } else {
                None
            }
        }
        "===" => node.first_argument(),
        "include?" | "cover?" => {
            let receiver = node.receiver()?;
            if deparenthesize(receiver).is_range() {
                node.first_argument()
            } else {
                None
            }
        }
        "match" | "match?" | "=~" => {
            let argument = node.first_argument();
            let receiver = node.receiver()?;
            if receiver.is(NodeType::Regexp) {
                argument
            } else if argument?.is(NodeType::Regexp) {
                Some(receiver)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn same(a: Option<Node<'_>>, b: Node<'_>) -> bool {
    a.is_some_and(|a| a.same_as(b))
}

fn collect_conditions<'a>(node: Node<'a>, target: Node<'a>, conditions: &mut Vec<Node<'a>>) -> bool {
    let condition = match node.ty() {
        NodeType::Begin => {
            return node
                .child(0)
                .is_some_and(|c| collect_conditions(c, target, conditions));
        }
        NodeType::Or => {
            return node
                .lhs()
                .is_some_and(|l| collect_conditions(l, target, conditions))
                && node
                    .rhs()
                    .is_some_and(|r| collect_conditions(r, target, conditions));
        }
        NodeType::MatchWithLvasgn => match (node.child(0), node.child(1)) {
            (Some(lhs), Some(rhs)) => condition_from_binary_op(lhs, rhs, target),
            _ => None,
        },
        NodeType::Send => condition_from_send_node(node, target),
        _ => None,
    };
    match condition {
        Some(condition) => {
            conditions.push(condition);
            true
        }
        None => false,
    }
}

fn condition_from_send_node<'a>(node: Node<'a>, target: Node<'a>) -> Option<Node<'a>> {
    match node.method_name()? {
        "is_a?" => {
            if same(node.receiver(), target) {
                node.first_argument()
            } else {
                None
            }
        }
        "==" | "eql?" | "equal?" => {
            let condition =
                condition_from_binary_op(node.receiver()?, node.first_argument()?, target)?;
            (!class_reference(condition)).then_some(condition)
        }
        "=~" | "match" | "match?" => {
            condition_from_binary_op(node.receiver()?, node.first_argument()?, target)
        }
        "===" => {
            if same(node.first_argument(), target) {
                node.receiver()
            } else {
                None
            }
        }
        "include?" | "cover?" => {
            let receiver = deparenthesize(node.receiver()?);
            (receiver.is_range() && same(node.first_argument(), target)).then_some(receiver)
        }
        _ => None,
    }
}

fn condition_from_binary_op<'a>(lhs: Node<'a>, rhs: Node<'a>, target: Node<'a>) -> Option<Node<'a>> {
    let lhs = deparenthesize(lhs);
    let rhs = deparenthesize(rhs);
    if lhs.same_as(target) {
        Some(rhs)
    } else if rhs.same_as(target) {
        Some(lhs)
    } else {
        None
    }
}

fn branch_conditions(node: Node<'_>) -> Vec<Node<'_>> {
    let mut conditions = Vec::new();
    let mut node = Some(node);
    while let Some(n) = node.filter(|n| n.is(NodeType::If) && !n.is_ternary()) {
        if let Some(condition) = n.condition() {
            conditions.push(condition);
        }
        node = n.else_branch();
    }
    conditions
}

fn const_reference(node: Node<'_>) -> bool {
    if !node.is(NodeType::Const) {
        return false;
    }
    let name = node.short_name().unwrap_or("");
    name.chars().count() > 1 && name == name.to_uppercase()
}

fn class_reference(node: Node<'_>) -> bool {
    node.is(NodeType::Const)
        && node
            .short_name()
            .is_some_and(|name| regex!(r"[[:lower:]]").find(name).is_some())
}

fn deparenthesize(mut node: Node<'_>) -> Node<'_> {
    while node.is(NodeType::Begin) {
        match node.child_nodes().last() {
            Some(last) => node = last,
            None => break,
        }
    }
    node
}

fn correction_range(node: Node<'_>) -> Range {
    let keyword = node.parent().and_then(|p| p.loc().keyword).unwrap();
    range_between(keyword.begin_pos, node.source_range().end_pos)
}

fn regexp_with_working_captures(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::MatchWithLvasgn => {
            node.loc_is(node.loc().selector, "=~")
                && node.child(0).is_some_and(regexp_with_named_captures)
        }
        NodeType::Send => {
            node.is_method("match")
                && [node.receiver(), node.first_argument()]
                    .into_iter()
                    .flatten()
                    .any(regexp_with_named_captures)
        }
        _ => false,
    }
}

fn regexp_with_named_captures(node: Node<'_>) -> bool {
    if !node.is(NodeType::Regexp) {
        return false;
    }
    let source: String = node
        .child_nodes()
        .filter(|c| c.is(NodeType::Str))
        .filter_map(|c| c.str_content())
        .collect();
    regex!(r"(?<!\\)\(\?(<[A-Za-z_]|'[A-Za-z_])").find(&source).is_some()
}
