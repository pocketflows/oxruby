//! Port of `lib/rubocop/cop/style/ascii_comments.rb`.

use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Comment;

const MSG: &str = "Use only ascii symbols in comments.";

#[derive(Default)]
pub struct AsciiComments;

impl Cop for AsciiComments {
    fn name(&self) -> &'static str {
        "Style/AsciiComments"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let allowed: Vec<String> = cx
            .cop_config_list("AllowedChars")
            .into_iter()
            .map(str::to_owned)
            .collect();
        let comments: Vec<Comment<'_>> = cx.processed_source.comments().to_vec();
        for comment in comments {
            if comment.text.is_ascii() {
                continue;
            }
            if only_allowed_non_ascii_chars(comment.text, &allowed) {
                continue;
            }
            let start = comment.range.begin_pos + first_non_ascii_index(comment.text);
            let length = comment.text[start - comment.range.begin_pos..]
                .char_indices()
                .find(|(_, c)| c.is_ascii())
                .map_or(comment.text.len() - (start - comment.range.begin_pos), |(i, _)| i);
            cx.add_offense(range_between(start, start + length), MSG);
        }
    }
}

fn first_non_ascii_index(text: &str) -> usize {
    text.char_indices()
        .find(|(_, c)| !c.is_ascii())
        .map_or(0, |(i, _)| i)
}

fn only_allowed_non_ascii_chars(text: &str, allowed: &[String]) -> bool {
    text.chars()
        .filter(|c| !c.is_ascii())
        .all(|c| allowed.iter().any(|a| a.chars().eq(std::iter::once(c))))
}
