//! Port of `lib/rubocop/cop/layout/comment_indentation.rb`.

use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{Comment, ProcessedSource};
use crate::regex;

#[derive(Default)]
pub struct CommentIndentation;

impl Cop for CommentIndentation {
    fn name(&self) -> &'static str {
        "Layout/CommentIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let comments = cx.processed_source.comments();
        for (ix, comment) in comments.iter().enumerate() {
            check(cx, comments, comment, ix);
        }
    }
}

fn autocorrect(
    cx: &Investigation<'_, '_>,
    corrector: &mut Corrector,
    comments: &[Comment<'_>],
    index: usize,
    column_delta: isize,
) {
    // autocorrect_preceding_comments
    let preceding: Vec<&Comment<'_>> = comments[..=index].iter().rev().collect();
    for pair in preceding.windows(2) {
        let (below, above) = (pair[0], pair[1]);
        if !should_correct(cx.processed_source, above, below) {
            break;
        }
        autocorrect_one(cx, corrector, above, column_delta);
    }
    autocorrect_one(cx, corrector, &comments[index], column_delta);
}

fn should_correct(ps: &ProcessedSource<'_>, preceding: &Comment<'_>, reference: &Comment<'_>) -> bool {
    if ps.column_of(preceding.range.begin_pos) != ps.column_of(reference.range.begin_pos) {
        return false;
    }
    if !own_line_comment(ps, preceding) {
        return false;
    }
    let first = ps.line_of(preceding.range.begin_pos) + 1;
    let last = ps.line_of(reference.range.begin_pos);
    (first..last).all(|line| blank(ps.lines()[line - 1]))
}

fn autocorrect_one(cx: &Investigation<'_, '_>, corrector: &mut Corrector, comment: &Comment<'_>, column_delta: isize) {
    alignment_corrector::correct(
        corrector,
        cx.processed_source,
        cx.config,
        Some(Target::Range(comment.range)),
        column_delta,
        false,
    );
}

fn check(cx: &mut Investigation<'_, '_>, comments: &[Comment<'_>], comment: &Comment<'_>, comment_index: usize) {
    let ps = cx.processed_source;
    if !own_line_comment(ps, comment) {
        return;
    }
    let next_line = line_after_comment(ps, comment);
    let mut correct_comment_indentation = correct_indentation(cx, next_line);
    let column = ps.column_of(comment.range.begin_pos);
    let column_delta = correct_comment_indentation as isize - column as isize;
    if column_delta == 0 {
        return;
    }
    if next_line.is_some_and(two_alternatives) {
        correct_comment_indentation += configured_indentation_width(cx);
        if column == correct_comment_indentation {
            return;
        }
    }
    if correctly_aligned_with_preceding_comment(cx, comments, comment_index, column) {
        return;
    }
    let message = format!(
        "Incorrect indentation detected (column {column} instead of {correct_comment_indentation})."
    );
    cx.add_offense_cx(comment.range, &message, |corrector, cx| {
        autocorrect(cx, corrector, comments, comment_index, column_delta);
    });
}

fn correctly_aligned_with_preceding_comment(
    cx: &Investigation<'_, '_>,
    comments: &[Comment<'_>],
    comment_index: usize,
    column: usize,
) -> bool {
    if !cx.cop_config_bool("AllowForAlignment", false) {
        return false;
    }
    let ps = cx.processed_source;
    for other in comments[..comment_index].iter().rev() {
        if !own_line_comment(ps, other) {
            return ps.column_of(other.range.begin_pos) == column;
        }
    }
    false
}

fn blank(line: &str) -> bool {
    line.chars().all(char::is_whitespace)
}

fn own_line_comment(ps: &ProcessedSource<'_>, comment: &Comment<'_>) -> bool {
    let own_line = ps.lines()[ps.line_of(comment.range.begin_pos) - 1];
    regex!(r"\A\s*#").find(own_line).is_some()
}

fn line_after_comment<'s>(ps: &ProcessedSource<'s>, comment: &Comment<'_>) -> Option<&'s str> {
    let lines = ps.lines();
    let line = ps.line_of(comment.range.begin_pos);
    lines.get(line..)?.iter().find(|l| !blank(l)).copied()
}

fn correct_indentation(cx: &Investigation<'_, '_>, next_line: Option<&str>) -> usize {
    let Some(next_line) = next_line else {
        return 0;
    };
    let indentation_of_next_line = next_line
        .chars()
        .position(|c| !c.is_whitespace())
        .unwrap_or(0);
    indentation_of_next_line
        + if less_indented(cx, next_line) {
            configured_indentation_width(cx)
        } else {
            0
        }
}

fn less_indented(cx: &Investigation<'_, '_>, line: &str) -> bool {
    let rule = cx
        .config
        .for_cop("Layout/AccessModifierIndentation")
        .get("EnforcedStyle")
        .and_then(serde_yaml_ng::Value::as_str)
        == Some("outdent");
    regex!(r"\A\s*(end\b|[)}\]])").find(line).is_some() || (rule && bare_access_modifier(line))
}

fn bare_access_modifier(line: &str) -> bool {
    regex!(r"\A\s*(private|protected|public)\s*(#.*)?\z").find(line).is_some()
}

fn two_alternatives(line: &str) -> bool {
    regex!(r"^\s*(else|elsif|when|in|rescue|ensure)\b").find(line).is_some()
}
