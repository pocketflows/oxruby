//! Port of `lib/rubocop/cop/mixin/documentation_comment.rb`.

use crate::ast::Node;
use crate::cop::Investigation;
use crate::cop::mixin::annotation_comment::AnnotationComment;
use crate::cop::mixin::util::comment_line;
use crate::directive_comment::DirectiveComment;
use crate::processed_source::Comment;
use crate::regex;

/// `documentation_comment?(node)`.
pub fn documentation_comment(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let ps = cx.processed_source;
    let preceding_lines = preceding_lines(cx, node);
    let Some(last) = preceding_lines.last() else {
        return false;
    };
    // `preceding_comment?(node, preceding_lines.last)`
    if node.first_line() != ps.line_of(last.range.begin_pos) + 1 || !comment_line(last.text) {
        return false;
    }
    let keywords = annotation_keywords(cx);
    preceding_lines.iter().any(|comment| {
        !AnnotationComment::new(*comment, &keywords).is_annotation()
            && !interpreter_directive_comment(comment)
            && !rubocop_directive_comment(comment)
    })
}

/// `preceding_lines(node)`: the node's comments on lines before it.
pub fn preceding_lines<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> Vec<Comment<'s>> {
    let ps = cx.processed_source;
    let line = node.first_line();
    ps.node_comments(node)
        .into_iter()
        .filter(|c| ps.line_of(c.range.begin_pos) < line)
        .copied()
        .collect()
}

fn interpreter_directive_comment(comment: &Comment<'_>) -> bool {
    regex!(r"^#\s*(frozen_string_literal|encoding):")
        .find(comment.text)
        .is_some()
}

fn rubocop_directive_comment(comment: &Comment<'_>) -> bool {
    DirectiveComment::new(comment.text, comment.range, 0).has_match()
}

fn annotation_keywords<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    match cx.config.for_cop("Style/CommentAnnotation").get("Keywords") {
        Some(serde_yaml_ng::Value::Sequence(items)) => items
            .iter()
            .filter_map(serde_yaml_ng::Value::as_str)
            .collect(),
        _ => Vec::new(),
    }
}
