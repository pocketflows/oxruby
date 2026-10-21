//! Port of `lib/rubocop/cop/layout/empty_lines_around_method_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::empty_lines_around_body::{Body, MSG_EXTRA};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const KIND: &str = "method";
const BODY: Body<'static> = Body {
    kind: KIND,
    style: "no_empty_lines",
};

#[derive(Default)]
pub struct EmptyLinesAroundMethodBody;

impl Cop for EmptyLinesAroundMethodBody {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundMethodBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_endless() {
            if !offending_endless_method(cx, node) {
                return;
            }
            register_offense_for_endless_method(cx, node);
        } else {
            let first_line = node
                .arguments_node()
                .filter(|args| args.has_source())
                .map(|args| args.last_line());
            BODY.check(cx, node, node.body(), first_line);
        }
    }
}

fn offending_endless_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let assignment_line = node.line_of(node.loc().assignment.unwrap());
    node.body().is_some_and(|body| body.first_line() > assignment_line + 1)
        && cx
            .processed_source
            .lines()
            .get(assignment_line)
            .is_some_and(|l| l.is_empty())
}

fn register_offense_for_endless_method(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let assignment_line = node.line_of(node.loc().assignment.unwrap());
    let line_range = cx.processed_source.line_range(assignment_line + 1);
    let range = Range::new(line_range.begin_pos, line_range.begin_pos + 1);
    let msg = BODY.message(MSG_EXTRA, "beginning");
    cx.add_offense_with_correction(range, &msg, |corrector| corrector.remove(range));
}
