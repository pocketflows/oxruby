//! Port of `lib/rubocop/cop/mixin/annotation_comment.rb` (the
//! `AnnotationComment` class).

use crate::cop::mixin::util::regexp_escape;
use crate::processed_source::Comment;

/// An annotation comment such as `# TODO: fix this`.
#[derive(Debug, Clone)]
pub struct AnnotationComment<'a> {
    pub comment: Comment<'a>,
    pub margin: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub colon: Option<&'a str>,
    pub space: Option<&'a str>,
    pub note: Option<&'a str>,
}

impl<'a> AnnotationComment<'a> {
    pub fn new(comment: Comment<'a>, keywords: &[&str]) -> Self {
        let mut sorted: Vec<&str> = keywords.to_vec();
        // `sort_by { |w| -w.length }` (stable for equal lengths).
        sorted.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        let union: Vec<String> = sorted.iter().map(|k| regexp_escape(k)).collect();
        // `Regexp.union([])` matches nothing.
        let union = if union.is_empty() {
            "(?!)".to_owned()
        } else {
            union.join("|")
        };
        let pattern = format!(r"^(# ?)(\b(?:{union})\b)(\s*:)?(\s+)?(\S+)?");
        let regex = onig::Regex::with_options(
            &pattern,
            onig::RegexOptions::REGEX_OPTION_IGNORECASE,
            onig::Syntax::ruby(),
        )
        .ok();
        let captures = regex.as_ref().and_then(|r| r.captures(comment.text));
        let at = |i: usize| {
            captures
                .as_ref()
                .and_then(|c| c.pos(i))
                .map(|(s, e)| &comment.text[s..e])
        };
        Self {
            comment,
            margin: at(1),
            keyword: at(2),
            colon: at(3),
            space: at(4),
            note: at(5),
        }
    }

    /// `annotation?`
    pub fn is_annotation(&self) -> bool {
        self.keyword_appearance() && !self.just_keyword_of_sentence()
    }

    /// `correct?(colon:)`
    pub fn is_correct(&self, colon: bool) -> bool {
        let (Some(keyword), Some(_), Some(_)) = (self.keyword, self.space, self.note) else {
            return false;
        };
        if keyword != keyword.to_uppercase() {
            return false;
        }
        self.colon.is_none() == !colon
    }

    /// `bounds`: the byte range of the keyword, colon and space.
    pub fn bounds(&self) -> (usize, usize) {
        let start = self.comment.range.begin_pos + self.margin.map_or(0, str::len);
        let length = [self.keyword, self.colon, self.space]
            .iter()
            .map(|e| e.map_or(0, str::len))
            .sum::<usize>();
        (start, start + length)
    }

    fn keyword_appearance(&self) -> bool {
        self.keyword.is_some() && (self.colon.is_some() || self.space.is_some())
    }

    fn just_keyword_of_sentence(&self) -> bool {
        let Some(keyword) = self.keyword else {
            return false;
        };
        keyword == capitalize(keyword)
            && self.colon.is_none()
            && self.space.is_some()
            && self.note.is_some()
    }
}

/// `String#capitalize`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}
