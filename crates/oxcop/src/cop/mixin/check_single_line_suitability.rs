//! Port of `lib/rubocop/cop/mixin/check_single_line_suitability.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::regex;

/// `max_line_length` (`AutocorrectLogic`): `Layout/LineLength`'s `Max` when
/// that cop is enabled.
pub fn max_line_length(cx: &Investigation<'_, '_>) -> Option<i64> {
    if !cx.config.cop_enabled("Layout/LineLength") {
        return None;
    }
    Some(
        cx.config
            .for_cop("Layout/LineLength")
            .get("Max")
            .and_then(serde_yaml_ng::Value::as_i64)
            .unwrap_or(120),
    )
}

/// `suitable_as_single_line?(node)`.
pub fn suitable_as_single_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !too_long(cx, node) && !comment_within(cx, node) && safe_to_split(node)
}

fn too_long(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(max) = max_line_length(cx) else {
        return false;
    };
    let lines = cx.processed_source.lines();
    let first = node.first_line().saturating_sub(1).min(lines.len());
    let last = node.last_line().min(lines.len());
    let source = lines[first..last].join("\n");
    to_single_line(&source).chars().count() as i64 > max
}

/// `to_single_line(source)`.
pub fn to_single_line(source: &str) -> String {
    let s =
        regex!(r#"" *\\\n\s*'"#).replace_all(source, |_: &onig::Captures<'_>| "\" + '".to_owned());
    let s = regex!(r#"' *\\\n\s*""#).replace_all(&s, |_: &onig::Captures<'_>| "' + \"".to_owned());
    let s = regex!(r#"(["']) *\\\n\s*\1"#).replace_all(&s, |_: &onig::Captures<'_>| String::new());
    let s = regex!(r"\n\s*(?=(&)?\.\w)").replace_all(&s, |_: &onig::Captures<'_>| String::new());
    regex!(r"\s*\\?\n\s*").replace_all(&s, |_: &onig::Captures<'_>| " ".to_owned())
}

fn comment_within(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let (first, last) = (node.first_line(), node.last_line());
    cx.processed_source.comments().iter().any(|comment| {
        let line = cx.processed_source.line_of(comment.range.begin_pos);
        first <= line && line <= last
    })
}

fn safe_to_split(node: Node<'_>) -> bool {
    use NodeType::*;
    !node
        .descendants()
        .any(|d| d.is_any(&[If, Case, Kwbegin, Rescue, Ensure]) || d.is_any_def())
        && !node.descendants().any(|d| {
            d.is_any(&[Dstr, Str])
                && (d.is_heredoc()
                    // `value.include?("\n")`, also for strings that aren't
                    // valid UTF-8 (a `dstr`'s parts are descendants too).
                    || (d.is(Str)
                        && d.children()
                            .first()
                            .and_then(|c| c.str_bytes())
                            .is_some_and(|b| b.contains(&b'\n'))))
        })
        && !node
            .descendants()
            .any(|d| d.is_any(&[Begin, Sym]) && d.is_multiline())
}
