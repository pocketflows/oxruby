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

/// `max_line_length` as the including cop sees it: `Layout/LineLength`
/// aliases it to its own `max` (`cop_config['Max']`, even when the
/// configuration disables the cop and `--only` runs it).
fn cop_max_line_length(cx: &Investigation<'_, '_>) -> Option<usize> {
    if cx.cop_name() == "Layout/LineLength" {
        as_usize(cx.cop_config_value("Max"))
    } else {
        max_line_length(cx)
    }
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
    let max = cop_max_line_length(cx).unwrap_or(usize::MAX);
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

    let max = cop_max_line_length(cx).unwrap_or(usize::MAX);
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

/// `URI::RFC3986_Parser::HOST` (uri 1.1.1), with `-` in group names
/// spelled `_` (Oniguruma doesn't allow them).
const RFC3986_HOST: &str = concat!(
    r"(?<IP_literal>\[(?:(?<IPv6address>(?:\h{1,4}:){6}(?<ls32>\h{1,4}:\h{1,4}",
    r"|(?<IPv4address>(?<dec_octet>[1-9]\d|1\d{2}|2[0-4]\d|25[0-5]|\d)",
    r"\.\g<dec_octet>\.\g<dec_octet>\.\g<dec_octet>))",
    r"|::(?:\h{1,4}:){5}\g<ls32>",
    r"|\h{1,4}?::(?:\h{1,4}:){4}\g<ls32>",
    r"|(?:(?:\h{1,4}:)?\h{1,4})?::(?:\h{1,4}:){3}\g<ls32>",
    r"|(?:(?:\h{1,4}:){,2}\h{1,4})?::(?:\h{1,4}:){2}\g<ls32>",
    r"|(?:(?:\h{1,4}:){,3}\h{1,4})?::\h{1,4}:\g<ls32>",
    r"|(?:(?:\h{1,4}:){,4}\h{1,4})?::\g<ls32>",
    r"|(?:(?:\h{1,4}:){,5}\h{1,4})?::\h{1,4}",
    r"|(?:(?:\h{1,4}:){,6}\h{1,4})?::)",
    r"|(?<IPvFuture>v\h++\.[!$&-.0-9:;=A-Z_a-z~]++))\])",
    r"|\g<IPv4address>",
    r"|(?<reg_name>(?:%\h\h|[!$&-.0-9;=A-Z_a-z~])*+)",
);
const RFC3986_USERINFO: &str = r"(?:%\h\h|[!$&-.0-9:;=A-Z_a-z~])*+";
const RFC3986_SCHEME: &str = r"[A-Za-z][+\-.0-9A-Za-z]*+";
const RFC3986_SEG: &str = r"(?:%\h\h|[!$&-.0-9:;=@A-Z_a-z~/])";
const RFC3986_SEG_NC: &str = r"(?:%\h\h|[!$&-.0-9;=@A-Z_a-z~])";
const RFC3986_FRAGMENT: &str = r"(?:%\h\h|[!$&-.0-9:;=@A-Z_a-z~/?])*+";

/// `URI::RFC3986_Parser::RFC3986_URI`.
fn rfc3986_uri() -> &'static onig::Regex {
    regex!(&format!(
        concat!(
            r"\A(?<seg>{seg}){{0}}(?<URI>(?<scheme>{scheme}):(?<hier_part>//",
            r"(?<authority>(?:(?<userinfo>{userinfo})@)?(?<host>{host})",
            r"(?::(?<port>\d*+))?)(?<path_abempty>(?:/\g<seg>*+)?)",
            r"|(?<path_absolute>/((?!/)\g<seg>++)?)|(?<path_rootless>(?!/)\g<seg>++)",
            r"|(?<path_empty>))(?:\?(?<query>[^\#]*+))?(?:\#(?<fragment>{fragment}))?)\z",
        ),
        seg = RFC3986_SEG,
        scheme = RFC3986_SCHEME,
        userinfo = RFC3986_USERINFO,
        host = RFC3986_HOST,
        fragment = RFC3986_FRAGMENT,
    ))
}

/// `URI::RFC3986_Parser::RFC3986_relative_ref`.
fn rfc3986_relative_ref() -> &'static onig::Regex {
    regex!(&format!(
        concat!(
            r"\A(?<seg>{seg}){{0}}(?<relative_ref>(?<relative_part>//",
            r"(?<authority>(?:(?<userinfo>{userinfo})@)?(?<host>{host}(?<!/))?",
            r"(?::(?<port>\d*+))?)(?<path_abempty>(?:/\g<seg>*+)?)",
            r"|(?<path_absolute>/\g<seg>*+)|(?<path_noscheme>{seg_nc}++(?:/\g<seg>*+)?)",
            r"|(?<path_empty>))(?:\?(?<query>[^\#]*+))?(?:\#(?<fragment>{fragment}))?)\z",
        ),
        seg = RFC3986_SEG,
        seg_nc = RFC3986_SEG_NC,
        userinfo = RFC3986_USERINFO,
        host = RFC3986_HOST,
        fragment = RFC3986_FRAGMENT,
    ))
}

/// `valid_uri?`: `URI.parse` (uri 1.1.1's RFC 3986 parser, then the
/// scheme's class) doesn't raise `URI::InvalidURIError`. `mailto` URIs it
/// can't handle raise `InvalidComponentError` upstream, which isn't rescued;
/// they count as invalid here.
fn valid_uri(uri_ish_string: &str) -> bool {
    let s = uri_ish_string;
    // `URI must be ascii only`.
    if !s.is_ascii() {
        return false;
    }
    let (scheme, rest) = if rfc3986_uri().find(s).is_some() {
        let (scheme, rest) = s.split_once(':').unwrap_or((s, ""));
        (Some(scheme.to_ascii_uppercase()), rest)
    } else if rfc3986_relative_ref().find(s).is_some() {
        (None, s)
    } else {
        return false;
    };
    let (before_fragment, fragment) = match rest.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (rest, None),
    };
    let (path, query) = match before_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_fragment, None),
    };
    // `path-rootless`: the opaque part, which takes the query along.
    let opaque = scheme.is_some() && !path.is_empty() && !path.starts_with('/');
    if !opaque
        && let Some(query) = query
        && regex!(r"%[^0-9A-Fa-f][^0-9A-Fa-f]").find(query).is_some()
    {
        // `query=`: `invalid percent escape`.
        return false;
    }
    match scheme.as_deref() {
        // `URI::LDAP#initialize`: no fragment, and a path (`parse_dn`).
        Some("LDAP" | "LDAPS") => fragment.is_none() && !opaque,
        // `URI::FTP#initialize`: `raise InvalidURIError unless path`.
        Some("FTP") => !opaque,
        Some("MAILTO") => {
            let opaque_part = if opaque {
                Some(match query {
                    Some(query) => format!("{path}?{query}"),
                    None => path.to_owned(),
                })
            } else {
                query.map(|query| format!("?{query}"))
            };
            opaque_part.is_some_and(|opaque| {
                let to = opaque.split('?').next().unwrap_or("");
                regex!(r"\A(?:[^@,;]+@[^@,;]+(?:\z|[,;]))*\z")
                    .find(to)
                    .is_some()
            })
        }
        _ => true,
    }
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

#[cfg(test)]
mod uri_tests {
    use super::valid_uri;

    #[test]
    fn valid_uri_is_uri_parse() {
        for (s, ok) in [
            ("LDAP::DEFAULT_GROUP_UNIQUE_MEMBER_LIST_KEY", false),
            ("LDAP:x", false),
            ("http://a b", false),
            ("ldap::foo", false),
            ("http:", true),
            ("http://example.com/a%zz", false),
            ("https://example.com/path?a=1#frag", true),
            ("https://example.com/?q=%zz", false),
            ("ftp:opaque", false),
            ("mailto:a@b.c", true),
            ("mailto:nobody", false),
            ("http://[::1]:80/x", true),
            ("ldap://host/dc=x#frag", false),
            ("otherprotocol://a.very.long/x", true),
            ("http://exämple.com", false),
            (
                "https://docs.sentry.io/platforms/ruby/tracing/instrumentation/performance-metrics/#supported-measurement-units",
                true,
            ),
            (
                "https://fonts.googleapis.com/css2?family=Inter:ital,opsz,wght@0,14..32,100..900;1,14..32,100..900&family=Noto+Serif:ital,wght@0,100..900;1,100..900&display=swap",
                true,
            ),
            ("https://example.com/link", true),
        ] {
            assert_eq!(valid_uri(s), ok, "{s}");
        }
    }
}
