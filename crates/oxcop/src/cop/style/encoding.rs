//! Port of `lib/rubocop/cop/style/encoding.rb`.

use crate::cop::mixin::range_help::{SurroundingSpace, Side, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::magic_comment::MagicComment;

const MSG: &str = "Unnecessary utf-8 encoding comment.";
const SHEBANG: &str = "#!";

#[derive(Default)]
pub struct Encoding;

impl Cop for Encoding {
    fn name(&self) -> &'static str {
        "Style/Encoding"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.buffer_source().is_empty() {
            return;
        }
        for (line_number, comment) in comments(ps.lines()) {
            if !offense(&comment) {
                continue;
            }
            let range = ps.line_range(line_number);
            let text = comment.without("encoding");
            let src = ps.buffer_source();
            cx.add_offense_with_correction(range, MSG, |corrector| {
                if text.trim().is_empty() {
                    corrector.remove(range_with_surrounding_space(
                        src,
                        range,
                        SurroundingSpace::side(Side::Right),
                    ));
                } else {
                    corrector.replace(range, text);
                }
            });
        }
    }
}

fn comments<'a>(lines: &[&'a str]) -> Vec<(usize, MagicComment<'a>)> {
    let mut comments = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with(SHEBANG) {
            continue;
        }
        let comment = MagicComment::parse(line);
        if !comment.valid() {
            return comments;
        }
        comments.push((index + 1, comment));
    }
    comments
}

fn offense(comment: &MagicComment<'_>) -> bool {
    comment
        .encoding()
        .is_some_and(|encoding| encoding.eq_ignore_ascii_case("utf-8"))
}
