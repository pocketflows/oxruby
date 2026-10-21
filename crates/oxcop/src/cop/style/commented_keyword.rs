//! Port of `lib/rubocop/cop/style/commented_keyword.rb`.

use crate::cop::mixin::range_help::{SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DIRECTIVE_COMMENT_REGEXP;
use crate::processed_source::Comment;
use crate::regex;

#[derive(Default)]
pub struct CommentedKeyword;

impl Cop for CommentedKeyword {
    fn name(&self) -> &'static str {
        "Style/CommentedKeyword"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let comments: Vec<Comment<'_>> = ps.comments().to_vec();
        for comment in comments {
            let line_number = ps.line_of(comment.range.begin_pos);
            let line = ps.source_line(line_number);
            if !offensive(line, &comment) {
                continue;
            }
            let Some(captures) = regex!(r"(?<keyword>\S+).*#").captures(line) else {
                continue;
            };
            let keyword = captures.at(1).unwrap_or("").to_owned();
            let message =
                format!("Do not place comments on the same line as the `{keyword}` keyword.");
            cx.add_offense_with_correction(comment.range, &message, |corrector| {
                let range = range_with_surrounding_space(
                    ps.buffer_source(),
                    comment.range,
                    SurroundingSpace {
                        newlines: false,
                        ..SurroundingSpace::default()
                    },
                );
                corrector.remove(range);
                if keyword != "end" {
                    corrector.insert_before(
                        ps.line_range(line_number),
                        format!("{}\n", comment.text),
                    );
                }
            });
        }
    }
}

fn offensive(line: &str, comment: &Comment<'_>) -> bool {
    if rbs_inline_annotation(line, comment) || steep_annotation(comment) {
        return false;
    }
    let keyword = [
        regex!(r"^\s*begin\s"),
        regex!(r"^\s*class\s"),
        regex!(r"^\s*def\s"),
        regex!(r"^\s*end\s"),
        regex!(r"^\s*module\s"),
    ]
    .iter()
    .any(|r| r.find(line).is_some());
    let allowed = regex!(r"#\s*:nodoc:").find(line).is_some()
        || regex!(r"#\s*:yields:").find(line).is_some()
        || DIRECTIVE_COMMENT_REGEXP.find(line).is_some();
    keyword && !allowed
}

fn rbs_inline_annotation(line: &str, comment: &Comment<'_>) -> bool {
    if regex!(r"\A\s*class\s+(\w|::)+\s*<\s*(\w|::)+")
        .find(line)
        .is_some()
    {
        regex!(r"\A#\[.+\]").find(comment.text).is_some()
    } else if regex!(r"\A\s*(def\s|end)").find(line).is_some() {
        comment.text.starts_with("#:")
    } else {
        false
    }
}

fn steep_annotation(comment: &Comment<'_>) -> bool {
    regex!(r"#\ssteep:ignore(\s|\z)").find(comment.text).is_some()
}
