//! Port of `lib/rubocop/cop/layout/empty_comment.rb`.

use crate::cop::mixin::range_help::{
    SurroundingSpace, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::{Cop, Investigation};
use crate::processed_source::{Comment, ProcessedSource};
use crate::regex;

const MSG: &str = "Source code comment is empty.";

#[derive(Default)]
pub struct EmptyComment;

impl Cop for EmptyComment {
    fn name(&self) -> &'static str {
        "Layout/EmptyComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let border = cx.cop_config_bool("AllowBorderComment", false);
        if cx.cop_config_bool("AllowMarginComment", false) {
            let comments = concat_consecutive_comments(ps, ps.comments());
            investigate(cx, border, comments);
        } else {
            for comment in ps.comments() {
                if !empty_comment_only(border, &comment_text(comment)) {
                    continue;
                }
                add(cx, comment);
            }
        }
    }
}

fn investigate(cx: &mut Investigation<'_, '_>, border: bool, comments: Vec<(String, Vec<&Comment<'_>>)>) {
    for (text, chunk) in comments {
        if !empty_comment_only(border, &text) {
            continue;
        }
        for offense_comment in chunk {
            add(cx, offense_comment);
        }
    }
}

fn add(cx: &mut Investigation<'_, '_>, comment: &Comment<'_>) {
    let ps = cx.processed_source;
    let comment = *comment;
    cx.add_offense_with_correction(comment.range, MSG, |corrector| {
        let range = if inline_comment(ps, &comment) {
            range_with_surrounding_space(
                ps.buffer_source(),
                comment.range,
                SurroundingSpace {
                    newlines: false,
                    ..SurroundingSpace::default()
                },
            )
        } else {
            range_by_whole_lines(ps, comment.range, true)
        };
        corrector.remove(range);
    });
}

fn concat_consecutive_comments<'c, 's>(
    ps: &ProcessedSource<'_>,
    comments: &'c [Comment<'s>],
) -> Vec<(String, Vec<&'c Comment<'s>>)> {
    let mut chunks: Vec<Vec<&Comment<'_>>> = Vec::new();
    for comment in comments {
        if let Some(last) = chunks.last_mut().and_then(|c| c.last().copied())
            && ps.line_of(last.range.begin_pos) + 1 == ps.line_of(comment.range.begin_pos)
            && ps.column_of(last.range.begin_pos) == ps.column_of(comment.range.begin_pos)
        {
            chunks.last_mut().unwrap().push(comment);
            continue;
        }
        chunks.push(vec![comment]);
    }
    chunks
        .into_iter()
        .map(|chunk| {
            let joined: String = chunk.iter().map(|c| comment_text(c)).collect();
            (joined, chunk)
        })
        .collect()
}

fn empty_comment_only(border: bool, comment_text: &str) -> bool {
    if border {
        regex!(r"\A(#\n)+\z").find(comment_text).is_some()
    } else {
        regex!(r"\A(#+\n)+\z").find(comment_text).is_some()
    }
}

fn comment_text(comment: &Comment<'_>) -> String {
    format!(
        "{}\n",
        comment
            .text
            .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '\x0b')
    )
}

fn inline_comment(ps: &ProcessedSource<'_>, comment: &Comment<'_>) -> bool {
    let line = ps.source_line(ps.line_of(comment.range.begin_pos));
    let line_start = ps.line_start(ps.line_of(comment.range.begin_pos));
    let preceding = &line[..comment.range.begin_pos - line_start];
    !preceding.trim().is_empty()
}
