//! Port of `lib/rubocop/cop/style/if_inside_else.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::if_then_corrector::IfThenCorrector;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{
    range_between, range_by_whole_lines, range_with_comments, range_with_comments_and_lines,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Convert `if` nested inside `else` to `elsif`.";

#[derive(Default)]
pub struct IfInsideElse {
    ignored: IgnoredNodes,
}

impl Cop for IfInsideElse {
    fn name(&self) -> &'static str {
        "Style/IfInsideElse"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_ternary() || node.is_unless() {
            return;
        }
        let Some(else_branch) = node
            .else_branch()
            .filter(|e| e.is(NodeType::If) && e.is_if())
        else {
            return;
        };
        if allow_if_modifier_in_else_branch(cx, else_branch) {
            return;
        }
        if comments_between_else_and_if(cx, node, else_branch) {
            return;
        }
        let ignored = &mut self.ignored;
        cx.add_offense_cx(else_branch.loc().keyword.unwrap(), MSG, |corrector, cx| {
            if ignored.part_of(node) {
                return;
            }
            autocorrect(cx.processed_source, corrector, else_branch);
            ignored.ignore(node);
        });
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.has_then() {
        // If the nested `if` is a then node, correct it first, then the next
        // pass will use `correct_to_elsif_from_if_inside_else_form`.
        IfThenCorrector::new(node, Some(0)).call(corrector);
        return;
    }
    if node.is_modifier_form() {
        correct_to_elsif_from_modifier_form(corrector, node);
    } else {
        correct_to_elsif_from_if_inside_else_form(ps, corrector, node, node.condition().unwrap());
    }
}

fn correct_to_elsif_from_modifier_form(corrector: &mut Corrector, node: Node<'_>) {
    let condition = node.condition().unwrap();
    corrector.replace(
        node.parent().unwrap().loc().else_.unwrap(),
        format!("elsif {}", condition.source()),
    );
    let condition_range = range_between(
        node.if_branch().unwrap().source_range().end_pos,
        condition.source_range().end_pos,
    );
    corrector.remove(condition_range);
}

fn correct_to_elsif_from_if_inside_else_form(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    condition: Node<'_>,
) {
    corrector.replace(
        node.parent().unwrap().loc().else_.unwrap(),
        format!("elsif {}", condition.source()),
    );
    let if_condition_range = if_condition_range(node, condition);
    if let Some(if_branch) = node.if_branch() {
        corrector.replace(
            if_condition_range,
            ps.slice(range_with_comments(ps, if_branch)),
        );
        corrector.remove(range_with_comments_and_lines(ps, if_branch));
    } else {
        corrector.remove(range_by_whole_lines(ps, if_condition_range, true));
    }
    corrector.remove(condition.source_range());
    if let Some(end_range) = find_end_range(Some(node)) {
        corrector.remove(range_by_whole_lines(ps, end_range, true));
    }
}

fn find_end_range(node: Option<Node<'_>>) -> Option<Range> {
    let node = node?;
    match node.loc().end {
        Some(end_range) => Some(end_range),
        None => find_end_range(node.parent()),
    }
}

fn if_condition_range(node: Node<'_>, condition: Node<'_>) -> Range {
    range_between(
        node.loc().keyword.unwrap().begin_pos,
        condition.source_range().end_pos,
    )
}

fn comments_between_else_and_if(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    else_branch: Node<'_>,
) -> bool {
    if else_branch.is_modifier_form() {
        return false;
    }
    let else_end = node.loc().else_.unwrap().end_pos;
    let if_begin = else_branch.loc().keyword.unwrap().begin_pos;
    cx.processed_source.comments().iter().any(|comment| {
        let comment_pos = comment.range.begin_pos;
        comment_pos > else_end && comment_pos < if_begin
    })
}

fn allow_if_modifier_in_else_branch(cx: &Investigation<'_, '_>, else_branch: Node<'_>) -> bool {
    cx.cop_config_bool("AllowIfModifier", false) && else_branch.is_modifier_form()
}
