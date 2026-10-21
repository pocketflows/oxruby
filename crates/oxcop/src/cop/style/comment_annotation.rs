//! Port of `lib/rubocop/cop/style/comment_annotation.rb`.

use crate::cop::mixin::annotation_comment::AnnotationComment;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Comment;

#[derive(Default)]
pub struct CommentAnnotation;

impl Cop for CommentAnnotation {
    fn name(&self) -> &'static str {
        "Style/CommentAnnotation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let keywords = cx.cop_config_list("Keywords");
        let requires_colon = cx.cop_config_bool("RequireColon", false);
        let comments: Vec<Comment<'_>> = ps.comments().to_vec();
        for (index, comment) in comments.iter().enumerate() {
            let line = |c: &Comment<'_>| ps.line_of(c.range.begin_pos);
            let first_comment_line = index == 0 || line(&comments[index - 1]) + 1 < line(comment);
            let inline_comment = !comment_line(ps.source_line(line(comment)));
            if !first_comment_line && !inline_comment {
                continue;
            }
            let annotation = AnnotationComment::new(*comment, &keywords);
            if !annotation.is_annotation() || annotation.is_correct(requires_colon) {
                continue;
            }
            let keyword = annotation.keyword.unwrap_or("");
            let message = if annotation.note.is_some() {
                if requires_colon {
                    format!(
                        "Annotation keywords like `{keyword}` should be all upper case, followed by a colon, and a space, then a note describing the problem."
                    )
                } else {
                    format!(
                        "Annotation keywords like `{keyword}` should be all upper case, followed by a space, then a note describing the problem."
                    )
                }
            } else {
                format!("Annotation comment, with keyword `{keyword}`, is missing a note.")
            };
            let (begin, end) = annotation.bounds();
            let range = range_between(begin, end);
            cx.add_offense_with_correction(range, &message, |corrector| {
                if annotation.note.is_none() {
                    return;
                }
                if requires_colon {
                    corrector.replace(range, format!("{}: ", keyword.to_uppercase()));
                } else {
                    corrector.replace(range, format!("{} ", keyword.to_uppercase()));
                }
            });
        }
    }
}
