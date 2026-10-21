//! Port of `lib/rubocop/cop/lint/else_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::indentation;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Odd `else` layout detected. Did you mean to use `elsif`?";

#[derive(Default)]
pub struct ElseLayout;

impl Cop for ElseLayout {
    fn name(&self) -> &'static str {
        "Lint/ElseLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_ternary() {
            return;
        }
        if node.has_then() && !node.else_branch().is_some_and(|e| e.is(NodeType::Begin)) {
            return;
        }
        // If the if is on a single line, it'll be handled by `Style/OneLineConditional`
        if node.is_single_line() {
            return;
        }
        check(cx, node);
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(else_branch) = node.else_branch() else {
        return;
    };
    if node.has_else() && node.loc_is(node.loc().else_, "else") {
        check_else(cx, node, else_branch);
    } else if node.is_if() {
        check(cx, else_branch);
    }
}

fn check_else<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, else_branch: Node<'a>) {
    let first_else = if else_branch.is(NodeType::Begin) {
        else_branch.child(0)
    } else {
        Some(else_branch)
    };
    let Some(first_else) = first_else else {
        return;
    };
    let else_loc = node.loc().else_.unwrap();
    if !same_line(cx.processed_source, first_else.source_range(), else_loc) {
        return;
    }
    let indentation = indentation(cx, node);
    cx.add_offense_with_correction(first_else.source_range(), MSG, |corrector| {
        corrector.insert_after(else_loc, "\n");
        let blank_range = range_between(else_loc.end_pos, first_else.source_range().begin_pos);
        corrector.replace(blank_range, indentation);
    });
}
