//! Port of `lib/rubocop/cop/layout/multiline_block_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_between, range_with_surrounding_space,
};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Block body expression is on the same line as the block start.";
const ARG_MSG: &str = "Block argument expression is not on the same line as the block start.";
const PIPE_SIZE: usize = 1;

#[derive(Default)]
pub struct MultilineBlockLayout;

impl Cop for MultilineBlockLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineBlockLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_single_line() {
            return;
        }
        if !(args_on_beginning_line(node) || line_break_necessary_in_args(cx, node)) {
            let args = node.arguments_node().unwrap();
            add_offense_for_expression(cx, node, args.source_range(), ARG_MSG);
        }
        let Some(body) = node.body() else {
            return;
        };
        if !same_line(cx.processed_source, node.loc().begin.unwrap(), body.source_range()) {
            return;
        }
        add_offense_for_expression(cx, node, body.source_range(), MSG);
    }
}

fn args_on_beginning_line(node: Node<'_>) -> bool {
    !node.has_arguments()
        || node.line_of(node.loc().begin.unwrap()) == node.arguments_node().unwrap().last_line()
}

fn line_break_necessary_in_args(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(max_line_length) = cx.max_line_length() else {
        return false;
    };
    needed_length_for_args(node) as i64 > max_line_length
}

fn needed_length_for_args(node: Node<'_>) -> usize {
    let first_line = node.source().split_inclusive('\n').next().unwrap_or("");
    node.column()
        + characters_needed_for_space_and_pipes(node)
        + first_line.trim_end_matches('\n').chars().count()
        + block_arg_string(node, node.arguments_node().unwrap())
            .chars()
            .count()
}

fn characters_needed_for_space_and_pipes(node: Node<'_>) -> usize {
    let first_line = node.source().split_inclusive('\n').next().unwrap_or("");
    if first_line.ends_with("|\n") {
        PIPE_SIZE
    } else {
        (PIPE_SIZE * 2) + 1
    }
}

fn add_offense_for_expression(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    expression: Range,
    msg: &str,
) {
    let range = range_between(expression.begin_pos, expression.end_pos);
    cx.add_offense_cx(range, msg, |corrector, cx| autocorrect(corrector, cx, node));
}

fn autocorrect(corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>) {
    let mut expr_before_body = None;
    if !args_on_beginning_line(node) {
        autocorrect_arguments(corrector, cx, node);
        expr_before_body = Some(node.arguments_node().unwrap().source_range().end());
    }
    let Some(body) = node.body() else {
        return;
    };
    let expr_before_body = expr_before_body.unwrap_or_else(|| node.loc().begin.unwrap());
    if !same_line(cx.processed_source, expr_before_body, body.source_range()) {
        return;
    }
    autocorrect_body(corrector, node, body);
}

fn autocorrect_arguments(corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>) {
    let args = node.arguments_node().unwrap();
    let end_pos = range_with_surrounding_space(
        cx.processed_source.buffer_source(),
        args.source_range(),
        SurroundingSpace {
            side: Side::Right,
            newlines: false,
            ..SurroundingSpace::default()
        },
    )
    .end_pos;
    let range = range_between(node.loc().begin.unwrap().end_pos, end_pos);
    corrector.replace(range, format!(" |{}|", block_arg_string(node, args)));
}

fn autocorrect_body(corrector: &mut Corrector, node: Node<'_>, block_body: Node<'_>) {
    let first_node = if block_body.is(NodeType::Begin) && !block_body.source().starts_with('(') {
        block_body.child(0).unwrap()
    } else {
        block_body
    };
    let block_start_col = node.column();
    corrector.insert_before(
        first_node.source_range(),
        format!("\n  {}", " ".repeat(block_start_col)),
    );
}

fn block_arg_string(node: Node<'_>, args: Node<'_>) -> String {
    let mut arg_string = args
        .child_nodes()
        .map(|arg| {
            if arg.is(NodeType::Mlhs) {
                format!("({})", block_arg_string(node, arg))
            } else {
                arg.source().to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if include_trailing_comma(node.arguments_node().unwrap()) {
        arg_string.push(',');
    }
    arg_string
}

fn include_trailing_comma(args: Node<'_>) -> bool {
    let arg_count = args.descendants().filter(|d| d.is(NodeType::Arg)).count();
    arg_count == 1 && args.source().contains(',')
}
