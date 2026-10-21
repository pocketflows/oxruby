//! Port of `lib/rubocop/cop/style/block_comments.rb`.

use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{Comment, Range};
use crate::regex;

const MSG: &str = "Do not use block comments.";
const BEGIN_LENGTH: usize = "=begin\n".len();
const END_LENGTH: usize = "\n=end".len();

#[derive(Default)]
pub struct BlockComments;

impl Cop for BlockComments {
    fn name(&self) -> &'static str {
        "Style/BlockComments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let comments: Vec<Comment<'_>> = cx.processed_source.comments().to_vec();
        for comment in comments {
            if !comment.is_document() {
                continue;
            }
            let ps = cx.processed_source;
            cx.add_offense_with_correction(comment.range, MSG, |corrector| {
                let (eq_begin, eq_end, contents) = parts(&comment);
                corrector.remove(eq_begin);
                if !contents.is_empty() {
                    let source = format!("# {}", ps.slice(contents)).replace("\n\n", "\n#\n");
                    let source = regex!(r"\n(?=[^#])").replace_all(&source, "\n# ");
                    corrector.replace(contents, source);
                }
                corrector.remove(eq_end);
            });
        }
    }
}

fn parts(comment: &Comment<'_>) -> (Range, Range, Range) {
    let expr = comment.range;
    let eq_begin = expr.resize(BEGIN_LENGTH);
    let eq_end = eq_end_part(comment, expr);
    let contents = range_between(eq_begin.end_pos, eq_end.begin_pos);
    (eq_begin, eq_end, contents)
}

fn eq_end_part(comment: &Comment<'_>, expr: Range) -> Range {
    if !comment.text.ends_with('\n') {
        range_between(expr.end_pos - END_LENGTH - 1, expr.end_pos - 2)
    } else {
        range_between(expr.end_pos - END_LENGTH, expr.end_pos)
    }
}
