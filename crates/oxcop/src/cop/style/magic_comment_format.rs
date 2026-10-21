//! Port of `lib/rubocop/cop/style/magic_comment_format.rb`.

use std::sync::LazyLock;

use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};
use crate::magic_comment::MagicComment;
use crate::processed_source::{Comment, Range};

/// `MagicComment::KEYWORDS` values.
const KEYWORDS: &[&str] = &[
    "(?:en)?coding",
    "frozen[_-]string[_-]literal",
    "rbs_inline",
    "warn[_-]indent",
    "shareable[_-]constant[_-]value",
    "typed",
];

/// `CommentRange::DIRECTIVE_REGEXP`: the union of the keywords, each
/// case-insensitive.
static DIRECTIVE_REGEXP: LazyLock<String> = LazyLock::new(|| {
    KEYWORDS
        .iter()
        .map(|k| format!("(?i-mx:{k})"))
        .collect::<Vec<_>>()
        .join("|")
});

/// `CommentRange::VALUE_REGEXP` (its `\s` is a space in a Ruby string).
static VALUE_REGEXP: LazyLock<onig::Regex> = LazyLock::new(|| {
    ruby_regex(&format!(
        "(?:(?-mix:{}): *)(.*?)(?=;|$)",
        *DIRECTIVE_REGEXP
    ))
    .unwrap()
});

static DIRECTIVE: LazyLock<onig::Regex> =
    LazyLock::new(|| ruby_regex(&DIRECTIVE_REGEXP).unwrap());

/// `CommentRange#directives`.
fn directives(comment: &Comment<'_>) -> Vec<Range> {
    let begin = comment.range.begin_pos;
    DIRECTIVE
        .find_iter(comment.text)
        .map(|(b, e)| Range::new(begin + b, begin + e))
        .collect()
}

/// `CommentRange#values`.
fn values(comment: &Comment<'_>) -> Vec<Range> {
    let begin = comment.range.begin_pos;
    VALUE_REGEXP
        .captures_iter(comment.text)
        .filter_map(|c| c.pos(1))
        .map(|(b, e)| Range::new(begin + b, begin + e))
        .collect()
}

#[derive(Default)]
pub struct MagicCommentFormat;

impl Cop for MagicCommentFormat {
    fn name(&self) -> &'static str {
        "Style/MagicCommentFormat"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.ast().is_none() {
            return;
        }
        let last_line = ps
            .tokens()
            .iter()
            .find(|t| t.ty != "tCOMMENT")
            .map(|t| ps.line_of(t.range.begin_pos));
        let comments: Vec<Comment<'_>> = ps
            .comments()
            .iter()
            .filter(|c| last_line.is_none_or(|l| ps.line_of(c.range.begin_pos) < l))
            .filter(|c| MagicComment::parse(c.text).valid())
            .copied()
            .collect();
        let directive_capitalization = capitalization(cx, "DirectiveCapitalization");
        let value_capitalization = capitalization(cx, "ValueCapitalization");
        let style = style(cx);
        for comment in comments {
            let issue_directives: Vec<Range> = directives(&comment)
                .into_iter()
                .filter(|d| {
                    let text = cx.source(*d);
                    text.contains(wrong_separator(style))
                        || wrong_capitalization(text, directive_capitalization)
                })
                .collect();
            let issue_values: Vec<Range> = values(&comment)
                .into_iter()
                .filter(|v| wrong_capitalization(cx.source(*v), value_capitalization))
                .collect();

            if !issue_directives.is_empty() {
                let msg = format!(
                    "Prefer {} case for magic comments.",
                    expected_style(directive_capitalization, style)
                );
                for directive in issue_directives {
                    let source = cx.source(directive);
                    let replacement = replace_capitalization(source, directive_capitalization)
                        .replace(wrong_separator(style), correct_separator(style));
                    cx.add_offense_with_correction(directive, &msg, |corrector| {
                        corrector.replace(directive, &replacement);
                    });
                }
            }
            if !issue_values.is_empty() {
                let msg = format!(
                    "Prefer {} for magic comment values.",
                    value_capitalization.unwrap_or("")
                );
                for value in issue_values {
                    let replacement =
                        replace_capitalization(cx.source(value), value_capitalization);
                    cx.add_offense_with_correction(value, &msg, |corrector| {
                        corrector.replace(value, &replacement);
                    });
                }
            }
        }
    }
}

fn expected_style(directive_capitalization: Option<&str>, style: &str) -> String {
    let joined = [directive_capitalization, Some(style)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    crate::regex!(r"_?case\b").replace_all(&joined, "")
}

fn wrong_separator(style: &str) -> &'static str {
    if style == "snake_case" { "-" } else { "_" }
}

fn correct_separator(style: &str) -> &'static str {
    if style == "snake_case" { "_" } else { "-" }
}

fn wrong_capitalization(text: &str, expected_case: Option<&str>) -> bool {
    match expected_case {
        Some("lowercase") => text != text.to_lowercase(),
        Some("uppercase") => text != text.to_uppercase(),
        _ => false,
    }
}

fn replace_capitalization(text: &str, style: Option<&str>) -> String {
    match style {
        Some("lowercase") => text.to_lowercase(),
        Some("uppercase") => text.to_uppercase(),
        _ => text.to_owned(),
    }
}

/// `directive_capitalization` / `value_capitalization`: raises on a value
/// that isn't one of `SupportedCapitalizations`.
fn capitalization<'a>(cx: &Investigation<'a, '_>, key: &str) -> Option<&'a str> {
    let style = cx
        .cop_config_str(key)
        .map(|s| s.strip_prefix(':').unwrap_or(s));
    if let Some(style) = style {
        let supported = cx.cop_config_list("SupportedCapitalizations");
        if !supported
            .iter()
            .any(|s| s.strip_prefix(':').unwrap_or(s) == style)
        {
            panic!("Unknown `{key}` {style} selected!");
        }
    }
    style
}
