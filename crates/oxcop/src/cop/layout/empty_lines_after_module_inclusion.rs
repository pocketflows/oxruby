//! Port of `lib/rubocop/cop/layout/empty_lines_after_module_inclusion.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::ProcessedSource;

const MSG: &str = "Add an empty line after module inclusion.";
const MODULE_INCLUSION_METHODS: &[&str] = &["include", "extend", "prepend"];

#[derive(Default)]
pub struct EmptyLinesAfterModuleInclusion;

impl Cop for EmptyLinesAfterModuleInclusion {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAfterModuleInclusion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        MODULE_INCLUSION_METHODS
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_some() || node.arguments().is_empty() {
            return;
        }
        if node
            .parent()
            .is_some_and(|p| p.is(NodeType::Send) || p.is_any_block() || p.is(NodeType::Array))
        {
            return;
        }
        let ps = cx.processed_source;
        if next_line_empty_or_enable_directive_comment(ps, node.last_line()) {
            return;
        }
        if !require_empty_line(next_line_node(node)) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let mut node_range = range_by_whole_lines(ps, node.source_range(), false);
            let next_line = ps.line_of(node_range.end_pos) + 1;
            if enable_directive_comment(ps, next_line) {
                node_range = ps.comment_at_line(next_line).unwrap().range;
            }
            corrector.insert_after(node_range, "\n");
        });
    }
}

fn next_line_empty_or_enable_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    line_empty(ps, line) || (enable_directive_comment(ps, line + 1) && line_empty(ps, line + 1))
}

fn enable_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    let Some(comment) = ps.comment_at_line(line) else {
        return false;
    };
    DirectiveComment::new(comment.text, comment.range, line as i64).enabled()
}

fn line_empty(ps: &ProcessedSource<'_>, line: usize) -> bool {
    ps.lines()
        .get(line)
        .is_none_or(|l| l.chars().all(char::is_whitespace))
}

fn require_empty_line(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|node| !allowed_method(node))
}

fn allowed_method(node: Node<'_>) -> bool {
    let node = if node.is_modifier_form() {
        match node.body() {
            Some(body) => body,
            None => return false,
        }
    } else {
        node
    };
    node.is_send() && node.method_name().is_some_and(|m| MODULE_INCLUSION_METHODS.contains(&m))
}

fn next_line_node(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if parent.is_any(&[NodeType::If, NodeType::Rescue, NodeType::Ensure]) {
        return None;
    }
    node.right_sibling()
}
