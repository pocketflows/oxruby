//! Port of the app's `ErbEmbeddedFormatter`
//! (`code/lib/tasks/support/erb_embedded_formatter.rb`): `.html.erb` is
//! formatted with the ERB plugin, except that `<script>`/`<style>` bodies are
//! handed to oxfmt; whole `.js.erb`/`.css.erb`/`.json.erb` files go to oxfmt
//! the same way. ERB tags inside oxfmt-bound code are swapped for
//! placeholders while oxfmt runs and restored afterwards.

use crate::erb;
use crate::oxfmt::{EmbedError, Language, Oxfmt};
use crate::parser::ParseError;

pub const MIN_ERB_TAG_WIDTH: usize = 40;

#[derive(Debug)]
pub enum Error {
    /// `SyntaxTree::Parser::ParseError`
    Parse(ParseError),
    LeftoverPlaceholder,
    OxfmtMissing(String),
    UnsupportedFile(String),
}

impl Error {
    /// The Ruby class name stree_new prints in `[error]` lines.
    pub fn class_name(&self) -> &'static str {
        match self {
            Error::Parse(_) => "SyntaxTree::Parser::ParseError",
            Error::LeftoverPlaceholder => "ErbEmbeddedFormatter::LeftoverPlaceholderError",
            Error::OxfmtMissing(_) => "ErbEmbeddedFormatter::OxfmtMissingError",
            Error::UnsupportedFile(_) => "ErbEmbeddedFormatter::UnsupportedFileError",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Error::Parse(e) => e.message.clone(),
            Error::LeftoverPlaceholder => {
                "a <script>/<style> body was matched but never re-emitted (inside a comment?)"
                    .to_string()
            }
            Error::OxfmtMissing(m) | Error::UnsupportedFile(m) => m.clone(),
        }
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}

impl From<EmbedError> for Error {
    fn from(e: EmbedError) -> Self {
        match e {
            EmbedError::Missing(m) => Error::OxfmtMissing(m),
        }
    }
}

/// `ErbEmbeddedFormatter.format(source, maxwidth, filepath:)`
pub fn format(
    source: &str,
    maxwidth: usize,
    filepath: &str,
    oxfmt: &Oxfmt,
) -> Result<String, Error> {
    let extension = erb_file_extension(filepath);
    let language = match extension.as_deref() {
        Some("js") => Some(Language::Js),
        Some("css") => Some(Language::Css),
        Some("json") => Some(Language::Json),
        _ => None,
    };
    if let Some(language) = language {
        let mut out = format_code(language, source, maxwidth, oxfmt)?.join("\n");
        out.push('\n');
        return Ok(out);
    }
    if !matches!(extension.as_deref(), None | Some("html")) {
        return Err(Error::UnsupportedFile(format!(
            "no formatter for .{} ({filepath})",
            format_args!("{}.erb", extension.unwrap_or_default())
        )));
    }
    format_document(source, maxwidth, oxfmt)
}

/// `filepath[/\.([a-z]+)\.erb\z/i, 1]&.downcase`
fn erb_file_extension(filepath: &str) -> Option<String> {
    let lower = filepath.to_ascii_lowercase();
    let stem = lower.strip_suffix(".erb")?;
    let dot = stem.rfind('.')?;
    let ext = &stem[dot + 1..];
    if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_lowercase()) {
        Some(ext.to_string())
    } else {
        None
    }
}

fn format_document(source: &str, maxwidth: usize, oxfmt: &Oxfmt) -> Result<String, Error> {
    let mut embeds: Vec<Vec<String>> = Vec::new();
    let mut substituted = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut search = 0;
    while let Some(m) = find_embed_tag(source, search) {
        let body = &source[m.body.0..m.body.1];
        let tag = &source[m.tag.0..m.tag.1];
        let attributes = &source[m.attributes.0..m.attributes.1];
        substituted.push_str(&source[cursor..m.start]);
        if body.trim().is_empty() {
            substituted.push_str(&source[m.start..m.end]);
        } else {
            embeds.push(format_embed(
                &tag.to_ascii_lowercase(),
                attributes,
                body,
                maxwidth,
                oxfmt,
            )?);
            substituted.push_str(&format!(
                "<{tag}{attributes}>__STREE_EMBED_{}__</{tag}>",
                embeds.len() - 1
            ));
        }
        cursor = m.end;
        search = m.end;
    }
    substituted.push_str(&source[cursor..]);

    let document = erb::parse(&substituted)?;
    let formatted = erb::format::format_document(&document, maxwidth, Some(embeds));
    if contains_leftover_embed(&formatted) {
        return Err(Error::LeftoverPlaceholder);
    }
    Ok(formatted)
}

/// `/__STREE_EMBED_\d+__/`
fn contains_leftover_embed(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find("__STREE_EMBED_") {
        let after = &rest[i + 14..];
        let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && after[digits..].starts_with("__") {
            return true;
        }
        rest = &rest[i + 14..];
    }
    false
}

struct EmbedMatch {
    start: usize,
    end: usize,
    tag: (usize, usize),
    attributes: (usize, usize),
    body: (usize, usize),
}

/// `EMBED_TAG_PATTERN`:
/// `%r{<(script|style)\b((?:<%.*?%>|"[^"]*"|'[^']*'|[^>"'])*)>(.*?)</\1\s*>}mi`
fn find_embed_tag(source: &str, from: usize) -> Option<EmbedMatch> {
    let bytes = source.as_bytes();
    let mut start = from;
    while start < bytes.len() {
                let offset = source[start..].find('<')?;
        let lt = start + offset;
        if let Some(m) = match_embed_at(source, lt) {
            return Some(m);
        }
        start = lt + 1;
    }
    None
}

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn match_embed_at(source: &str, lt: usize) -> Option<EmbedMatch> {
    let rest = &source[lt + 1..];
    let name = if starts_with_ci(rest, "script") {
        "script"
    } else if starts_with_ci(rest, "style") {
        "style"
    } else {
        return None;
    };
    let tag_start = lt + 1;
    let tag_end = tag_start + name.len();
    // \b
    if source[tag_end..].chars().next().is_some_and(is_word_char) {
        return None;
    }
    // ((?:<%.*?%>|"[^"]*"|'[^']*'|[^>"'])*)>
    let bytes = source.as_bytes();
    let mut i = tag_end;
    loop {
        if i >= bytes.len() {
            return None;
        }
        if source[i..].starts_with("<%") {
            match source[i + 2..].find("%>") {
                Some(end) => {
                    i = i + 2 + end + 2;
                    continue;
                }
                None => {
                    i += 1;
                    continue;
                }
            }
        }
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i] as char;
                                let end = source[i + 1..].find(quote)?;
                i = i + 1 + end + 1;
            }
            b'>' => break,
            _ => {
                i += source[i..].chars().next().map_or(1, |c| c.len_utf8());
            }
        }
    }
    let attributes = (tag_end, i);
    let body_start = i + 1;
    // (.*?)</\1\s*>  (case-insensitive back reference)
    let mut j = body_start;
    while let Some(offset) = source[j..].find("</") {
        let close = j + offset;
        let after = &source[close + 2..];
        let tag_text = &source[tag_start..tag_end];
        if after.len() >= tag_text.len()
            && after.as_bytes()[..tag_text.len()].eq_ignore_ascii_case(tag_text.as_bytes())
        {
            let mut k = close + 2 + tag_text.len();
            while k < bytes.len()
                && matches!(bytes[k], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\x0b')
            {
                k += 1;
            }
            if bytes.get(k) == Some(&b'>') {
                return Some(EmbedMatch {
                    start: lt,
                    end: k + 1,
                    tag: (tag_start, tag_end),
                    attributes,
                    body: (body_start, close),
                });
            }
        }
        j = close + 2;
    }
    None
}

fn format_embed(
    tag: &str,
    attributes: &str,
    body: &str,
    maxwidth: usize,
    oxfmt: &Oxfmt,
) -> Result<Vec<String>, Error> {
    match embed_language(tag, attributes) {
        None => Ok(dedent(body)),
        Some(language) => format_code(language, body, maxwidth, oxfmt),
    }
}

/// `embed_language`
fn embed_language(tag: &str, attributes: &str) -> Option<Language> {
    if tag == "style" {
        return Some(Language::Css);
    }
    let script_type = script_type(attributes).map(|t| t.to_ascii_lowercase());
    match script_type.as_deref() {
        None
        | Some("")
        | Some("module")
        | Some("text/javascript")
        | Some("application/javascript") => Some(Language::Js),
        Some("application/json") | Some("application/ld+json") | Some("importmap") => {
            Some(Language::Json)
        }
        _ => None,
    }
}

/// `/\btype\s*=\s*["']?([^"'\s>]*)/i`
fn script_type(attributes: &str) -> Option<&str> {
    let lower = attributes.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = lower[from..].find("type") {
        let at = from + offset;
        let boundary = attributes[..at]
            .chars()
            .last()
            .is_none_or(|c| !is_word_char(c));
        if boundary {
            let mut i = at + 4;
            let bytes = attributes.as_bytes();
            while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'=') {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
                    i += 1;
                }
                if matches!(bytes.get(i), Some(b'"' | b'\'')) {
                    i += 1;
                }
                let value_start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b'"' | b'\'' | b'>')
                    && !(bytes[i] as char).is_ascii_whitespace()
                {
                    i += 1;
                }
                return Some(&attributes[value_start..i]);
            }
        }
        from = at + 4;
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlaceholderStyle {
    Identifier,
    String,
}

/// `format_code`
fn format_code(
    language: Language,
    body: &str,
    maxwidth: usize,
    oxfmt: &Oxfmt,
) -> Result<Vec<String>, Error> {
    let original_lines = dedent(body);
    let joined = original_lines.join("\n");
    let mut chosen: Option<(String, Vec<String>)> = None;
    for style in [PlaceholderStyle::Identifier, PlaceholderStyle::String] {
        let (code, erb_tags) = substitute_erb_tags(&joined, style);
        if let Some(result) = oxfmt.format(&code, language)?
            && placeholder_signature(&result) == placeholder_signature(&code)
        {
            chosen = Some((result, erb_tags));
            break;
        }
    }
    let Some((formatted, erb_tags)) = chosen else {
        return Ok(original_lines);
    };
    let restored = restore_erb_tags(&formatted, &erb_tags, maxwidth);
    if find_placeholder(&restored, 0, true).is_some() {
        return Ok(original_lines);
    }
    Ok(dedent(&restored))
}

/// `ERB_TAG_PATTERN` = `/<%(?!%).*?%>/m`: the ERB tags of a piece of code.
fn erb_tags(code: &str) -> Vec<(usize, usize)> {
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(offset) = code[from..].find("<%") {
        let start = from + offset;
        if code[start + 2..].starts_with('%') {
            from = start + 1;
            continue;
        }
        match code[start + 2..].find("%>") {
            Some(end) => {
                let end = start + 2 + end + 2;
                tags.push((start, end));
                from = end;
            }
            None => break,
        }
    }
    tags
}

fn substitute_erb_tags(code: &str, style: PlaceholderStyle) -> (String, Vec<String>) {
    let mut out = String::with_capacity(code.len());
    let mut tags = Vec::new();
    let mut cursor = 0;
    for (start, end) in erb_tags(code) {
        out.push_str(&code[cursor..start]);
        let tag = &code[start..end];
        let placeholder = erb_placeholder(tags.len(), tag, &code[..start], &code[end..], style);
        tags.push(tag.to_string());
        out.push_str(&placeholder);
        cursor = end;
    }
    out.push_str(&code[cursor..]);
    (out, tags)
}

/// `erb_placeholder`
fn erb_placeholder(
    index: usize,
    tag: &str,
    pre: &str,
    post: &str,
    style: PlaceholderStyle,
) -> String {
    let output_tag = tag.starts_with("<%=");
    // pre_match =~ /(?:\A|\n)[ \t]*\z/
    let line_start = pre.rfind('\n').map_or(0, |i| i + 1);
    let before_alone = pre[line_start..].chars().all(|c| c == ' ' || c == '\t');
    // post_match =~ /\A[ \t]*(?:\r?\n|\z)/
    let trimmed = post.trim_start_matches([' ', '\t']);
    let after_alone =
        trimmed.is_empty() || trimmed.starts_with('\n') || trimmed.starts_with("\r\n");
    let alone_on_line = before_alone && after_alone;

    if !output_tag || alone_on_line {
        format!("/*__ERB_{index}__*/")
    } else if quoted_in_source(pre, post) {
        format!("__ERB_{index}__")
    } else if style == PlaceholderStyle::String || contains_word(tag, "to_json") {
        format!("\"__ERB_STR_{index}__\"")
    } else {
        format!("__ERB_{index}__")
    }
}

/// `/\bword\b/`
fn contains_word(haystack: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(word) {
        let at = from + offset;
        let before = haystack[..at].chars().last();
        let after = haystack[at + word.len()..].chars().next();
        if before.is_none_or(|c| !is_word_char(c)) && after.is_none_or(|c| !is_word_char(c)) {
            return true;
        }
        from = at + word.len();
    }
    false
}

fn quoted_in_source(pre: &str, post: &str) -> bool {
    match pre.chars().last() {
        Some(quote @ ('"' | '\'' | '`')) => post.starts_with(quote),
        _ => false,
    }
}

/// A placeholder found by `ERB_PLACEHOLDER_PATTERN`
/// (`%r{/\*__ERB_(\d+)__\*/|"__ERB_STR_(\d+)__"|__ERB_(\d+)__}`): its byte
/// range and index digits. With `leftover`, matches
/// `LEFTOVER_ERB_PLACEHOLDER_PATTERN` (`/__ERB_(?:STR_)?\d+__/`) instead.
fn find_placeholder(code: &str, from: usize, leftover: bool) -> Option<(usize, usize, String)> {
    let bytes = code.as_bytes();
    let digits_at =
        |i: usize| -> usize { bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count() };
    let mut i = from;
    while i < bytes.len() {
        if leftover {
            if code[i..].starts_with("__ERB_") {
                let mut j = i + 6;
                if code[j..].starts_with("STR_") {
                    j += 4;
                }
                let d = digits_at(j);
                if d > 0 && code[j + d..].starts_with("__") {
                    return Some((i, j + d + 2, code[j..j + d].to_string()));
                }
            }
        } else {
            if code[i..].starts_with("/*__ERB_") {
                let j = i + 8;
                let d = digits_at(j);
                if d > 0 && code[j + d..].starts_with("__*/") {
                    return Some((i, j + d + 4, code[j..j + d].to_string()));
                }
            }
            if code[i..].starts_with("\"__ERB_STR_") {
                let j = i + 11;
                let d = digits_at(j);
                if d > 0 && code[j + d..].starts_with("__\"") {
                    return Some((i, j + d + 3, code[j..j + d].to_string()));
                }
            }
            if code[i..].starts_with("__ERB_") {
                let j = i + 6;
                let d = digits_at(j);
                if d > 0 && code[j + d..].starts_with("__") {
                    return Some((i, j + d + 2, code[j..j + d].to_string()));
                }
            }
        }
        i += code[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    None
}

/// `placeholder_signature`: placeholder order and the net brace depth of
/// the code between them.
fn placeholder_signature(code: &str) -> (Vec<String>, Vec<i64>) {
    let mut indices = Vec::new();
    let mut depths = Vec::new();
    let mut cursor = 0;
    let depth = |s: &str| s.matches('{').count() as i64 - s.matches('}').count() as i64;
    while let Some((start, end, digits)) = find_placeholder(code, cursor, false) {
        indices.push(digits);
        depths.push(depth(&code[cursor..start]));
        cursor = end;
    }
    depths.push(depth(&code[cursor..]));
    (indices, depths)
}

fn restore_erb_tags(code: &str, erb_tags: &[String], maxwidth: usize) -> String {
    let mut out = String::with_capacity(code.len());
    for line in code.split_inclusive('\n') {
        let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
        let indent = &line[..indent_len];
        let mut cursor = 0;
        while let Some((start, end, digits)) = find_placeholder(line, cursor, false) {
            out.push_str(&line[cursor..start]);
            let index: usize = digits.parse().unwrap_or(usize::MAX);
            match erb_tags.get(index) {
                Some(tag) => {
                    let width = (maxwidth as i64 - indent.chars().count() as i64)
                        .max(MIN_ERB_TAG_WIDTH as i64);
                    let formatted = format_erb_tag(tag, width as usize);
                    out.push_str(&formatted.replace('\n', &format!("\n{indent}")));
                }
                // `erb_tags[n]` is nil: `format_erb_tag(nil, ...)` raises
                // inside the parser; leave the placeholder so the caller
                // falls back to the original lines.
                None => out.push_str(&line[start..end]),
            }
            cursor = end;
        }
        out.push_str(&line[cursor..]);
    }
    out
}

/// `format_erb_tag`
fn format_erb_tag(tag: &str, width: usize) -> String {
    match erb::parse(tag) {
        Ok(document) => {
            let formatted =
                erb::format::format_document(&document, width.max(MIN_ERB_TAG_WIDTH), None);
            chomp(&formatted).to_string()
        }
        Err(_) => tag.to_string(),
    }
}

fn chomp(s: &str) -> &str {
    if let Some(stripped) = s.strip_suffix("\r\n") {
        stripped
    } else if let Some(stripped) = s.strip_suffix('\n') {
        stripped
    } else {
        s.strip_suffix('\r').unwrap_or(s)
    }
}

/// `dedent`
pub fn dedent(body: &str) -> Vec<String> {
    // body.split("\n", -1).map(&:rstrip); "".split returns []
    let mut lines: Vec<String> = if body.is_empty() {
        vec![]
    } else {
        body.split('\n')
            .map(|l| ruby_rstrip(l).to_string())
            .collect()
    };
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let indent = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.chars().take_while(|&c| c == ' ' || c == '\t').count())
        .min()
        .unwrap_or(0);
    lines
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                line
            } else {
                line.chars().skip(indent).collect()
            }
        })
        .collect()
}

/// `String#rstrip`: trailing whitespace and NULs.
fn ruby_rstrip(s: &str) -> &str {
    s.trim_end_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_script_tags() {
        let source = "<div><script type=\"module\">\n  a < b\n</script ></div>";
        let m = find_embed_tag(source, 0).unwrap();
        assert_eq!(&source[m.tag.0..m.tag.1], "script");
        assert_eq!(&source[m.attributes.0..m.attributes.1], " type=\"module\"");
        assert_eq!(&source[m.body.0..m.body.1], "\n  a < b\n");
        assert_eq!(&source[m.end..], "</div>");
    }

    #[test]
    fn placeholders() {
        let (code, tags) = substitute_erb_tags(
            "var a = <%= x %>;\n<% if y %>\nfoo(\"<%= z %>\")",
            PlaceholderStyle::Identifier,
        );
        assert_eq!(
            code,
            "var a = __ERB_0__;\n/*__ERB_1__*/\nfoo(\"__ERB_2__\")"
        );
        assert_eq!(tags.len(), 3);
        assert_eq!(
            placeholder_signature("{ /*__ERB_1__*/ } \"__ERB_STR_0__\""),
            (vec!["1".to_string(), "0".to_string()], vec![1, -1, 0])
        );
    }

    #[test]
    fn dedents() {
        assert_eq!(dedent("\n    a\n      b  \n\n"), vec!["a", "  b"]);
    }
}
