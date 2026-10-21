//! Port of `lib/rubocop/cop/mixin/line_length_help.rb`: helpers for
//! deciding whether a line is too long (`Layout/LineLength` and the
//! `StatementModifier` cops).
//!
//! Lengths and positions are in characters, as in Ruby. Ranges returned by
//! `find_excessive_range` are character ranges within the line (plus the
//! tab indentation difference), like Ruby's `begin_position...end_position`.

use std::ops::Range as CharRange;

use serde_yaml_ng::{Mapping, Value};

use crate::cop::Investigation;
use crate::cop::mixin::ruby_regex;
use crate::directive_comment::{DIRECTIVE_COMMENT_REGEXP, DirectiveComment};
use crate::regex;

fn line_length_config<'a>(cx: &Investigation<'a, '_>) -> &'a Mapping {
    cx.config.for_cop("Layout/LineLength")
}

/// Ruby truthiness of an optional config value.
fn truthy(value: Option<&Value>) -> bool {
    !matches!(value, None | Some(Value::Null) | Some(Value::Bool(false)))
}

fn as_usize(value: Option<&Value>) -> Option<usize> {
    value.and_then(Value::as_u64).map(|v| v as usize)
}

/// `Base#max_line_length` (`autocorrect_logic.rb`): `Layout/LineLength`'s
/// `Max` when that cop is enabled.
pub fn max_line_length(cx: &Investigation<'_, '_>) -> Option<usize> {
    if !cx.config.cop_enabled("Layout/LineLength") {
        return None;
    }
    Some(as_usize(line_length_config(cx).get("Max")).unwrap_or(120))
}

pub fn allow_rbs_inline_annotation(cx: &Investigation<'_, '_>) -> bool {
    truthy(line_length_config(cx).get("AllowRBSInlineAnnotation"))
}

/// `rbs_inline_annotation_on_source_line?(line_index)` (0-based index).
pub fn rbs_inline_annotation_on_source_line(cx: &Investigation<'_, '_>, line_index: usize) -> bool {
    let Some(comment) = cx.processed_source.comment_at_line(line_index + 1) else {
        return false;
    };
    regex!(r"\A(?:#:|#\[.+\]|#\|)").find(comment.text).is_some()
}

pub fn allow_cop_directives(cx: &Investigation<'_, '_>) -> bool {
    // Backward compatibility with the deprecated `IgnoreCopDirectives`.
    match line_length_config(cx).get("IgnoreCopDirectives") {
        Some(Value::Bool(false)) => return false,
        value if truthy(value) => return true,
        _ => {}
    }
    truthy(line_length_config(cx).get("AllowCopDirectives"))
}

/// `directive_on_source_line?(line_index)` (0-based index).
pub fn directive_on_source_line(cx: &Investigation<'_, '_>, line_index: usize) -> bool {
    let Some(comment) = cx.processed_source.comment_at_line(line_index + 1) else {
        return false;
    };
    DirectiveComment::new(comment.text, comment.range, 0).has_match()
}

pub fn allow_uri(cx: &Investigation<'_, '_>) -> bool {
    truthy(line_length_config(cx).get("AllowURI"))
}

pub fn allow_qualified_name(cx: &Investigation<'_, '_>) -> bool {
    truthy(line_length_config(cx).get("AllowQualifiedName"))
}

pub fn allowed_position(cx: &Investigation<'_, '_>, line: &str, range: &CharRange<usize>) -> bool {
    let max = max_line_length(cx).unwrap_or(usize::MAX);
    range.start < max && range.end == line_length(cx, line)
}

pub fn line_length(cx: &Investigation<'_, '_>, line: &str) -> usize {
    line.chars().count() + indentation_difference(cx, line)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExcessiveType {
    Uri,
    QualifiedName,
}

pub fn find_excessive_range(
    cx: &Investigation<'_, '_>,
    line: &str,
    ty: ExcessiveType,
) -> Option<CharRange<usize>> {
    let matches = match ty {
        ExcessiveType::Uri => match_uris(cx, line),
        ExcessiveType::QualifiedName => match_qualified_names(line),
    };
    let (begin_position, end_position) = *matches.last()?;
    let end_position = extend_end_position(line, end_position);

    let difference = indentation_difference(cx, line);
    let begin_position = begin_position + difference;
    let end_position = end_position + difference;

    let max = max_line_length(cx).unwrap_or(usize::MAX);
    if begin_position < max && end_position < max {
        return None;
    }
    Some(begin_position..end_position)
}

fn char_pos(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// Matches of `uri_regexp` as character offsets (`String#scan`).
fn match_uris(cx: &Investigation<'_, '_>, string: &str) -> Vec<(usize, usize)> {
    let Some(re) = uri_regexp(cx) else {
        return Vec::new();
    };
    re.find_iter(string)
        .filter(|&(b, e)| valid_uri(&string[b..e]))
        .map(|(b, e)| (char_pos(string, b), char_pos(string, e)))
        .collect()
}

fn match_qualified_names(string: &str) -> Vec<(usize, usize)> {
    regex!(r"\b(?:[A-Z][A-Za-z0-9_]*::)+[A-Za-z_][A-Za-z0-9_]*\b")
        .find_iter(string)
        .map(|(b, e)| (char_pos(string, b), char_pos(string, e)))
        .collect()
}

pub fn indentation_difference(cx: &Investigation<'_, '_>, line: &str) -> usize {
    let width = tab_indentation_width(cx);
    let index = if line.chars().next().is_some_and(|c| c != '\t') {
        0
    } else {
        // `line.index(/[^\t]/) || 0`
        line.char_indices()
            .find(|&(_, c)| c != '\t')
            .map_or(0, |(i, _)| i)
    };
    index * (width.saturating_sub(1))
}

/// The suffix of `line` from character `from`, clamped like Ruby's
/// `line[from..len]` (`None` past the end).
fn char_suffix(line: &str, from: usize) -> Option<&str> {
    let total = line.chars().count();
    if from > total {
        return None;
    }
    let byte = line.char_indices().nth(from).map_or(line.len(), |(i, _)| i);
    Some(&line[byte..])
}

fn extend_end_position(line: &str, mut end_position: usize) -> usize {
    // Extend the end position YARD comments with linked URLs of the form
    // {<uri> <title>}.
    if regex!(r"{(\s|\S)*}$").find(line).is_some()
        && let Some(rest) = char_suffix(line, end_position)
        && let Some((_, e)) = regex!(r"(\s|\S)*}").find(rest)
    {
        end_position += char_pos(rest, e);
    }

    // Extend the end position until the start of the next word, if any.
    if let Some(rest) = char_suffix(line, end_position)
        && let Some((_, e)) = regex!(r"^\S+(?=\s|$)").find(rest)
    {
        end_position += char_pos(rest, e);
    }
    end_position
}

/// `Layout/IndentationStyle`'s `IndentationWidth`, else
/// `Alignment#configured_indentation_width`.
pub fn tab_indentation_width(cx: &Investigation<'_, '_>) -> usize {
    as_usize(
        cx.config
            .for_cop("Layout/IndentationStyle")
            .get("IndentationWidth"),
    )
    .unwrap_or_else(|| configured_indentation_width(cx))
}

/// `Alignment#configured_indentation_width`.
pub fn configured_indentation_width(cx: &Investigation<'_, '_>) -> usize {
    as_usize(cx.cop_config_value("IndentationWidth"))
        .or_else(|| as_usize(cx.config.for_cop("Layout/IndentationWidth").get("Width")))
        .unwrap_or(2)
}

/// `URI::RFC2396_PARSER.pattern[:X_ABS_URI]` (uri 0.13, an extended regexp),
/// which is also the source of `make_regexp` without schemes
/// (`ABS_URI_REF`).
const X_ABS_URI: &str = r#"
        ([a-zA-Z][\-+.a-zA-Z\d]*):                           (?# 1: scheme)
        (?:
           ((?:[\-_.!~*'()a-zA-Z\d;?:@&=+$,]|%[a-fA-F\d]{2})(?:[\-_.!~*'()a-zA-Z\d;/?:@&=+$,\[\]]|%[a-fA-F\d]{2})*)                    (?# 2: opaque)
        |
           (?:(?:
             //(?:
                 (?:(?:((?:[\-_.!~*'()a-zA-Z\d;:&=+$,]|%[a-fA-F\d]{2})*)@)?        (?# 3: userinfo)
                   (?:((?:(?:[a-zA-Z0-9\-.]|%\h\h)+|\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}|\[(?:(?:[a-fA-F\d]{1,4}:)*(?:[a-fA-F\d]{1,4}|\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})|(?:(?:[a-fA-F\d]{1,4}:)*[a-fA-F\d]{1,4})?::(?:(?:[a-fA-F\d]{1,4}:)*(?:[a-fA-F\d]{1,4}|\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}))?)\]))(?::(\d*))?))? (?# 4: host, 5: port)
               |
                 ((?:[\-_.!~*'()a-zA-Z\d$,;:@&=+]|%[a-fA-F\d]{2})+)                 (?# 6: registry)
               )
             |
             (?!//))                           (?# XXX: '//' is the mark for hostport)
             (/(?:[\-_.!~*'()a-zA-Z\d:@&=+$,]|%[a-fA-F\d]{2})*(?:;(?:[\-_.!~*'()a-zA-Z\d:@&=+$,]|%[a-fA-F\d]{2})*)*(?:/(?:[\-_.!~*'()a-zA-Z\d:@&=+$,]|%[a-fA-F\d]{2})*(?:;(?:[\-_.!~*'()a-zA-Z\d:@&=+$,]|%[a-fA-F\d]{2})*)*)*)?                    (?# 7: path)
           )(?:\?((?:[\-_.!~*'()a-zA-Z\d;/?:@&=+$,\[\]]|%[a-fA-F\d]{2})*))?                 (?# 8: query)
        )
        (?:\#((?:[\-_.!~*'()a-zA-Z\d;/?:@&=+$,\[\]]|%[a-fA-F\d]{2})*))?                  (?# 9: fragment)
      "#;

/// `Regexp.escape` for a scheme name.
fn regexp_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '.' | '*' | '?' | '+' | '^' | '$' | '|' | '(' | ')' | '[' | ']' | '{' | '}' | '\\'
            | '/' | '-' => {
                out.push('\\');
                out.push(c);
            }
            ' ' => out.push_str("\\ "),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// `uri_regexp`: `URI::RFC2396_PARSER.make_regexp(schemes)`, with the
/// schemes required in the exact case given (`URISchemes`).
pub fn uri_regexp(cx: &Investigation<'_, '_>) -> Option<onig::Regex> {
    let pattern = match line_length_config(cx).get("URISchemes") {
        Some(Value::Sequence(items)) => {
            let schemes: Vec<String> = items
                .iter()
                .filter_map(Value::as_str)
                .map(regexp_escape)
                .collect();
            // `Regexp.union(*schemes)` (`(?!)` when empty), interpolated
            // twice: once by `make_regexp` and once by `uri_regexp`.
            let union = if schemes.is_empty() {
                "(?-mix:(?!))".to_owned()
            } else {
                format!("(?-mix:{})", schemes.join("|"))
            };
            format!("(?={union}:)(?x-mi:(?={union}:){X_ABS_URI})")
        }
        _ => format!("(?x-mi:{X_ABS_URI})"),
    };
    ruby_regex(&pattern)
}

/// `valid_uri?`: `URI.parse` succeeds. Every match of `uri_regexp` is an
/// absolute RFC 2396 URI, which `URI.parse` (the RFC 2396 parser in uri
/// 0.13) accepts.
fn valid_uri(_uri_ish_string: &str) -> bool {
    true
}

/// `line_length_without_directive`:
/// `DirectiveComment.before_comment(line).rstrip.length`.
pub fn line_length_without_directive(line: &str) -> usize {
    let before = match DIRECTIVE_COMMENT_REGEXP.find(line) {
        Some((b, _)) => &line[..b],
        None => line,
    };
    before.trim_end().chars().count()
}
