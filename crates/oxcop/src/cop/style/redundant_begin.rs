//! Port of `lib/rubocop/cop/style/redundant_begin.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    SurroundingSpace, range_between, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Redundant `begin` block detected.";

#[derive(Default)]
pub struct RedundantBegin;

impl Cop for RedundantBegin {
    fn name(&self) -> &'static str {
        "Style/RedundantBegin"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/BlockDelimiters"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Def,
            NodeType::Defs,
            NodeType::If,
            NodeType::Case,
            NodeType::CaseMatch,
            NodeType::While,
            NodeType::Until,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Kwbegin,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            Def | Defs => {
                let Some(body) = node.body().filter(|b| b.is(Kwbegin)) else {
                    return;
                };
                if node.is_endless() {
                    return;
                }
                register_offense(cx, body);
            }
            If => {
                if node.is_modifier_form() {
                    return;
                }
                inspect_branches(cx, node);
            }
            Case | CaseMatch => inspect_branches(cx, node),
            While | Until => {
                if node.is_modifier_form() {
                    return;
                }
                let Some(body) = node.body().filter(|b| b.is(Kwbegin)) else {
                    return;
                };
                if rescue_or_ensure_node(body) {
                    return;
                }
                register_offense(cx, body);
            }
            Block | Numblock | Itblock => {
                if cx.target_ruby_version() < 2.5 {
                    return;
                }
                if node.send_node().is_some_and(|s| s.is_lambda_literal()) || node.has_braces() {
                    return;
                }
                let Some(body) = node.body().filter(|b| b.is(Kwbegin)) else {
                    return;
                };
                register_offense(cx, body);
            }
            Kwbegin => {
                let Some(target_node) = node
                    .each_node()
                    .filter(|n| n.is(Kwbegin) && !allowable_kwbegin(*n))
                    .last()
                else {
                    return;
                };
                register_offense(cx, target_node);
            }
            _ => {}
        }
    }
}

/// `rescue_node || ensure_node` of a `kwbegin`.
fn rescue_or_ensure_node(node: Node<'_>) -> bool {
    node.child(0)
        .is_some_and(|first| matches!(first.ty(), NodeType::Rescue | NodeType::Ensure))
}

fn allowable_kwbegin(node: Node<'_>) -> bool {
    node.children().is_empty()
        || (node.parent().is_some() && node.children().len() >= 2)
        || node
            .child(0)
            .is_some_and(|f| matches!(f.ty(), NodeType::Rescue | NodeType::Ensure))
        || valid_context_using_only_begin(node)
}

fn valid_context_using_only_begin(node: Node<'_>) -> bool {
    let parent = node.parent();
    (parent.is_some_and(|p| p.is_assignment()) && node.children().len() != 1)
        || parent.is_some_and(|p| {
            p.is_post_condition_loop() || p.is_send() || p.is_operator_keyword()
        })
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let offense_range = node.loc().begin.unwrap();
    let ps = cx.processed_source;
    cx.add_offense_with_correction(offense_range, MSG, |corrector| {
        if node.parent().is_some_and(|p| p.is_assignment()) {
            replace_begin_with_statement(corrector, offense_range, node);
        } else {
            remove_begin(ps, corrector, offense_range, node);
        }
        if use_modifier_form_after_multiline_begin_block(node) {
            correct_modifier_form_after_multiline_begin_block(ps, corrector, node);
        }
        corrector.remove(node.loc().end.unwrap());
    });
}

fn replace_begin_with_statement(corrector: &mut Corrector, offense_range: Range, node: Node<'_>) {
    let first_child = node.child(0).unwrap();
    let mut source = first_child.source().to_owned();
    if first_child.is(NodeType::If) && first_child.is_modifier_form() {
        source = format!("({source})");
    }
    corrector.replace(offense_range, source);
    corrector.remove(range_between(
        offense_range.end_pos,
        first_child.source_range().end_pos,
    ));
    // Restore comments between `begin` and the first child above the
    // assignment line.
    let comments = node.range_source(range_between(
        offense_range.end_pos,
        first_child.source_range().begin_pos,
    ));
    if !comments.trim().is_empty() {
        corrector.insert_before(node.parent().unwrap().source_range(), comments);
    }
}

fn remove_begin(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    offense_range: Range,
    node: Node<'_>,
) {
    let mut offense_range = offense_range;
    if node.parent().is_some_and(|p| p.is_any_def() && p.is_endless()) {
        offense_range = range_with_surrounding_space(
            ps.buffer_source(),
            offense_range,
            SurroundingSpace::default(),
        );
    }
    corrector.remove(offense_range);
}

fn use_modifier_form_after_multiline_begin_block(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    node.is_multiline() && parent.is(NodeType::If) && parent.is_modifier_form()
}

fn correct_modifier_form_after_multiline_begin_block(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
) {
    let parent = node.parent().unwrap();
    let condition_range = range_between(
        parent.loc().keyword.unwrap().begin_pos,
        parent.condition().unwrap().source_range().end_pos,
    );
    corrector.insert_after(
        node.child(0).unwrap().source_range(),
        format!(" {}", node.range_source(condition_range)),
    );
    corrector.remove(range_by_whole_lines(ps, condition_range, true));
}

fn inspect_branches<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    for branch in node.branches().into_iter().flatten() {
        if !branch.is(NodeType::Kwbegin) || rescue_or_ensure_node(branch) {
            continue;
        }
        register_offense(cx, branch);
    }
}
