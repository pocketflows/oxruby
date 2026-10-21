//! Port of `lib/rubocop/cop/correctors/line_break_corrector.rb`.

use crate::ast::Node;
use crate::ast::lexer::Token;
use crate::cop::Corrector;
use crate::cop::mixin::trailing_body::{first_part_of, last_child};
use crate::processed_source::{Comment, ProcessedSource, Range};

/// `correct_trailing_body(configured_width:, corrector:, node:,
/// processed_source:)`.
pub fn correct_trailing_body(
    configured_width: usize,
    corrector: &mut Corrector,
    node: Node<'_>,
    ps: &ProcessedSource<'_>,
) {
    let Some(last) = last_child(node) else {
        return;
    };
    let range = first_part_of(last);
    let eol_comment = ps.comment_at_line(node.first_line());
    break_line_before(range, node, corrector, configured_width, 1);
    move_comment(eol_comment, node, corrector);
    remove_semicolon(ps, node, corrector);
}

/// `break_line_before(range:, node:, corrector:, configured_width:,
/// indent_steps:)`.
pub fn break_line_before(
    range: Range,
    node: Node<'_>,
    corrector: &mut Corrector,
    configured_width: usize,
    indent_steps: usize,
) {
    let column = node.column_of(node.loc().keyword.unwrap().begin_pos);
    corrector.insert_before(
        range,
        format!("\n{}", " ".repeat(column + indent_steps * configured_width)),
    );
}

/// `move_comment(eol_comment:, node:, corrector:)`.
pub fn move_comment(eol_comment: Option<&Comment<'_>>, node: Node<'_>, corrector: &mut Corrector) {
    let Some(eol_comment) = eol_comment else {
        return;
    };
    let column = node.column_of(node.loc().keyword.unwrap().begin_pos);
    corrector.insert_before(
        node.source_range(),
        format!("{}\n{}", eol_comment.text, " ".repeat(column)),
    );
    corrector.remove(eol_comment.range);
}

fn remove_semicolon(ps: &ProcessedSource<'_>, node: Node<'_>, corrector: &mut Corrector) {
    if let Some(semicolon) = semicolon(ps, node) {
        corrector.remove(semicolon.range);
    }
}

fn semicolon<'p>(ps: &'p ProcessedSource<'_>, node: Node<'_>) -> Option<&'p Token> {
    let body = node.body()?;
    ps.sorted_tokens()
        .iter()
        .filter(|t| t.ty == "tSEMI")
        .find(|token| {
            if token.range.end_pos <= node.source_range().begin_pos {
                return false;
            }
            ps.line_of(token.range.begin_pos) == body.first_line()
                && ps.column_of(token.range.begin_pos) < body.column()
        })
}
