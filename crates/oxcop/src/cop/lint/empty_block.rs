//! Port of `lib/rubocop/cop/lint/empty_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::Comment;

const MSG: &str = "Empty block detected.";

#[derive(Default)]
pub struct EmptyBlock;

impl Cop for EmptyBlock {
    fn name(&self) -> &'static str {
        "Lint/EmptyBlock"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.body().is_some() {
            return;
        }
        if allow_empty_lambdas(cx) && node.is_lambda_or_proc() {
            return;
        }
        if cx.cop_config_bool("AllowComments", false) && allow_comment(cx, node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn allow_comment(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let ps = cx.processed_source;
    if !ps.contains_comment(node.source_range()) {
        return false;
    }
    let line_comment = ps.comment_at_line(node.first_line());
    match line_comment {
        None => true,
        Some(comment) => !comment_disables_cop(cx, comment),
    }
}

fn allow_empty_lambdas(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowEmptyLambdas", false)
}

fn comment_disables_cop(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    let directive = DirectiveComment::new(
        comment.text,
        comment.range,
        cx.processed_source.line_of(comment.range.begin_pos) as i64,
    );
    directive.disabled()
        && directive
            .cop_names(cx.config.known_cops())
            .iter()
            .any(|name| name == cx.cop_name())
}
