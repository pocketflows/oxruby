//! Port of `lib/rubocop/cop/lint/loop.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::indent;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Use `Kernel#loop` with `break` rather than `begin/end/until`(or `while`).";

#[derive(Default)]
pub struct Loop;

impl Cop for Loop {
    fn name(&self) -> &'static str {
        "Lint/Loop"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::WhilePost, NodeType::UntilPost]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        register_offense(cx, node);
    }
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let body = node.body().unwrap();
    cx.add_offense_with_correction(node.loc().keyword.unwrap(), MSG, |corrector| {
        corrector.replace(body.loc().begin.unwrap(), "loop do");
        corrector.remove(keyword_and_condition_range(node));
        corrector.insert_before(body.loc().end.unwrap(), build_break_line(node));
    });
}

fn keyword_and_condition_range(node: Node<'_>) -> Range {
    let end = node.body().unwrap().loc().end.unwrap().end();
    end.join(node.source_range().end())
}

fn build_break_line(node: Node<'_>) -> String {
    let conditional_keyword = if node.is(NodeType::WhilePost) {
        "unless"
    } else {
        "if"
    };
    format!(
        "break {conditional_keyword} {}\n{}",
        node.condition().unwrap().source(),
        indent(node, 0)
    )
}
