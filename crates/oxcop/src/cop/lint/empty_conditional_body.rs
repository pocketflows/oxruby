//! Port of `lib/rubocop/cop/lint/empty_conditional_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::{comments_contain_disables, contains_comments};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct EmptyConditionalBody;

impl Cop for EmptyConditionalBody {
    fn name(&self) -> &'static str {
        "Lint/EmptyConditionalBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.body().is_some() || same_line(cx.processed_source, node.loc().begin, node.loc().end)
        {
            return;
        }
        if allow_comments(cx, node) {
            return;
        }
        let range = offense_range(node);
        let message = format!("Avoid `{}` branches without a body.", node.keyword());
        cx.add_offense_cx(range, &message, |corrector, cx| {
            if !can_simplify_conditional(node) {
                return;
            }
            flip_orphaned_else(cx.processed_source, corrector, node);
        });
    }
}

fn same_line(ps: &ProcessedSource<'_>, a: Option<Range>, b: Option<Range>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => ps.line_of(a.begin_pos) == ps.line_of(b.begin_pos),
        _ => false,
    }
}

fn allow_comments(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowComments", false)
        && contains_comments(cx.processed_source, node)
        && !comments_contain_disables(cx, node, cx.cop_name())
}

fn offense_range(node: Node<'_>) -> Range {
    match node.loc().else_ {
        Some(else_) => node.source_range().begin().join(else_.begin()),
        None => node.source_range(),
    }
}

fn can_simplify_conditional(node: Node<'_>) -> bool {
    node.else_branch().is_some() && node.loc_is(node.loc().else_, "else")
}

fn remove_empty_branch(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let Some(branch_range) = branch_range(node) else {
        return;
    };
    let range = if empty_if_branch(node) && else_branch(node) {
        branch_range
    } else {
        deletion_range(ps, branch_range)
    };
    corrector.remove(range);
}

fn flip_orphaned_else(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    corrector.replace(
        node.loc().else_.unwrap(),
        format!(
            "{} {}",
            node.inverse_keyword(),
            node.condition().unwrap().source()
        ),
    );
    remove_empty_branch(ps, corrector, node);
}

fn empty_if_branch(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !parent.is(NodeType::If) {
        return true;
    }
    let Some(if_branch) = parent.if_branch() else {
        return true;
    };
    if_branch.is(NodeType::If) && if_branch.body().is_none()
}

fn else_branch(node: Node<'_>) -> bool {
    node.else_branch().is_some_and(|e| !e.is(NodeType::If))
}

fn branch_range(node: Node<'_>) -> Option<Range> {
    if empty_if_branch(node) && else_branch(node) {
        Some(
            node.source_range()
                .with_end(node.loc().else_.unwrap().begin_pos),
        )
    } else if node.loc().else_.is_some() {
        Some(
            node.source_range()
                .with_end(node.condition().unwrap().source_range().end_pos),
        )
    } else {
        None
    }
}

fn deletion_range(ps: &ProcessedSource<'_>, range: Range) -> Range {
    let last_line = ps.line_of(range.end_pos);
    let line_start = ps.line_start(last_line);
    let line_len = ps.source_line(last_line).len();
    let last_column = range.end_pos - line_start;
    let end_offset = line_len - last_column + 1;
    let adjusted = Range::new(range.begin_pos, range.end_pos + end_offset);
    adjusted
        .intersect(Range::new(0, ps.buffer_source().len()))
        .unwrap_or(adjusted)
}
