//! Port of `lib/rubocop/cop/layout/space_around_block_parameters.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style_for;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct SpaceAroundBlockParameters;

impl Cop for SpaceAroundBlockParameters {
    fn name(&self) -> &'static str {
        "Layout/SpaceAroundBlockParameters"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(arguments) = node.arguments_node() else {
            return;
        };
        if !node.has_arguments() || pipes(arguments).is_none() {
            return;
        }
        if !conflicting_space_inside_parens_style(cx, arguments) {
            check_inside_pipes(cx, arguments);
        }
        if node.body().is_some() {
            check_after_closing_pipe(cx, arguments);
        }
        check_each_arg(cx, arguments);
    }
}

fn pipes(arguments: Node<'_>) -> Option<(Range, Range)> {
    Some((arguments.loc().begin?, arguments.loc().end?))
}

fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, "EnforcedStyleInsidePipes")
}

fn conflicting_space_inside_parens_style(cx: &Investigation<'_, '_>, arguments: Node<'_>) -> bool {
    if arguments.range_source(arguments.loc().begin.unwrap()) != "(" {
        return false;
    }
    // `config.for_enabled_cop('Layout/SpaceInsideParens')`.
    let parens = cx.config.for_cop("Layout/SpaceInsideParens");
    let enabled = !matches!(
        parens.get("Enabled"),
        None | Some(Value::Bool(false)) | Some(Value::Null)
    );
    if !enabled {
        return false;
    }
    match parens.get("EnforcedStyle").and_then(Value::as_str) {
        Some("no_space") => style(cx) == "space",
        Some("space" | "compact") => style(cx) == "no_space",
        _ => false,
    }
}

fn check_inside_pipes(cx: &mut Investigation<'_, '_>, arguments: Node<'_>) {
    match style(cx) {
        "no_space" => check_no_space_style_inside_pipes(cx, arguments),
        "space" => check_space_style_inside_pipes(cx, arguments),
        _ => {}
    }
}

fn check_after_closing_pipe(cx: &mut Investigation<'_, '_>, arguments: Node<'_>) {
    let (_opening_pipe, closing_pipe) = pipes(arguments).unwrap();
    let block = arguments.parent().unwrap();
    check_space(
        cx,
        closing_pipe.end_pos,
        block.body().unwrap().source_range().begin_pos,
        closing_pipe,
        "after closing `|`",
        None,
    );
}

fn check_no_space_style_inside_pipes(cx: &mut Investigation<'_, '_>, arguments: Node<'_>) {
    let args: Vec<Node<'_>> = arguments.child_nodes().collect();
    let (opening_pipe, closing_pipe) = pipes(arguments).unwrap();
    let first = args[0].source_range();
    let last = args.last().unwrap().source_range();
    check_no_space(cx, opening_pipe.end_pos, first.begin_pos, "Space before first");
    check_no_space(
        cx,
        last_end_pos_inside_pipes(arguments, last),
        closing_pipe.begin_pos,
        "Space after last",
    );
}

fn check_space_style_inside_pipes(cx: &mut Investigation<'_, '_>, arguments: Node<'_>) {
    let (opening_pipe, closing_pipe) = pipes(arguments).unwrap();
    check_opening_pipe_space(cx, arguments, opening_pipe);
    check_closing_pipe_space(cx, arguments, closing_pipe);
}

fn check_opening_pipe_space(
    cx: &mut Investigation<'_, '_>,
    arguments: Node<'_>,
    opening_pipe: Range,
) {
    let first_arg = arguments.child_nodes().next().unwrap();
    let range = first_arg.source_range();
    check_space(
        cx,
        opening_pipe.end_pos,
        range.begin_pos,
        range,
        "before first block parameter",
        Some(range),
    );
    check_no_space(
        cx,
        opening_pipe.end_pos,
        range.begin_pos.saturating_sub(1),
        "Extra space before first",
    );
}

fn check_closing_pipe_space(
    cx: &mut Investigation<'_, '_>,
    arguments: Node<'_>,
    closing_pipe: Range,
) {
    let last = arguments.child_nodes().last().unwrap().source_range();
    let last_end_pos = last_end_pos_inside_pipes(arguments, last);
    check_space(
        cx,
        last_end_pos,
        closing_pipe.begin_pos,
        last,
        "after last block parameter",
        None,
    );
    check_no_space(
        cx,
        last_end_pos + 1,
        closing_pipe.begin_pos,
        "Extra space after last",
    );
}

fn last_end_pos_inside_pipes(arguments: Node<'_>, range: Range) -> usize {
    let pos = range.end_pos;
    let num = pos - arguments.source_range().begin_pos;
    match arguments.source()[num..].find(',') {
        Some(trailing_comma_index) => pos + trailing_comma_index + 1,
        None => pos,
    }
}

fn check_each_arg(cx: &mut Investigation<'_, '_>, args: Node<'_>) {
    for arg in args.child_nodes() {
        check_arg(cx, arg);
    }
}

fn check_arg(cx: &mut Investigation<'_, '_>, arg: Node<'_>) {
    if arg.is(NodeType::Mlhs) {
        for a in arg.child_nodes() {
            check_arg(cx, a);
        }
    }
    let expr = arg.source_range();
    let begin = range_with_surrounding_space(
        cx.processed_source.buffer_source(),
        expr,
        SurroundingSpace::side(Side::Left),
    )
    .begin_pos;
    check_no_space(cx, begin, expr.begin_pos.saturating_sub(1), "Extra space before");
}

/// `check_space(space_begin_pos, space_end_pos, range, msg, node)`; `node`
/// is the range of the node to insert before, if any.
fn check_space(
    cx: &mut Investigation<'_, '_>,
    space_begin_pos: usize,
    space_end_pos: usize,
    range: Range,
    msg: &str,
    node: Option<Range>,
) {
    if space_begin_pos != space_end_pos {
        return;
    }
    let target = node.unwrap_or(range);
    let message = format!("Space {msg} missing.");
    cx.add_offense_with_correction(target, &message, |corrector| {
        if node.is_some() {
            corrector.insert_before(target, " ");
        } else {
            corrector.insert_after(target, " ");
        }
    });
}

fn check_no_space(
    cx: &mut Investigation<'_, '_>,
    space_begin_pos: usize,
    space_end_pos: usize,
    msg: &str,
) {
    if space_begin_pos >= space_end_pos {
        return;
    }
    let range = range_between(space_begin_pos, space_end_pos);
    if cx.source(range).contains('\n') {
        return;
    }
    let message = format!("{msg} block parameter detected.");
    cx.add_offense_with_correction(range, &message, |corrector| corrector.remove(range));
}
