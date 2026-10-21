//! Port of `lib/rubocop/cop/style/regexp_literal.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG_USE_SLASHES: &str = "Use `//` around regular expression.";
const MSG_USE_PERCENT_R: &str = "Use `%r` around regular expression.";

#[derive(Default)]
pub struct RegexpLiteral;

impl Cop for RegexpLiteral {
    fn name(&self) -> &'static str {
        "Style/RegexpLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let slash = slash_literal(node);
        if slash && percent_r_delimiters_conflict(cx, node) {
            return;
        }
        let message = if slash {
            (!allowed_slash_literal(cx, node)).then_some(MSG_USE_PERCENT_R)
        } else {
            (!allowed_percent_r_literal(cx, node)).then_some(MSG_USE_SLASHES)
        };
        let Some(message) = message else {
            return;
        };
        let (open, close) = calculate_replacement(cx, node);
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            corrector.replace(node.loc().begin.unwrap(), &open);
            corrector.replace(node.loc().end.unwrap(), &close);
            // correct_inner_slashes
            let regexp_begin = node.loc().begin.unwrap().end_pos;
            let before = inner_slash_for(node.range_source(node.loc().begin.unwrap()));
            let after = inner_slash_for(&open);
            for index in inner_slash_indices(node, before) {
                let start = regexp_begin + index;
                corrector.replace(range_between(start, start + before.len()), after);
            }
        });
    }
}

fn percent_r_delimiters_conflict(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let (opening, closing) = preferred_delimiters(cx);
    let pattern = match (opening.as_str(), closing.as_str()) {
        ("(", ")") => regex!(r"\\.|[()]"),
        ("[", "]") => regex!(r"\\.|[\[\]]"),
        ("{", "}") => regex!(r"\\.|[{}]"),
        ("<", ">") => regex!(r"\\.|[<>]"),
        _ => return false,
    };
    let text = node_body(node, false);
    let mut depth = 0i64;
    for (b, e) in pattern.find_iter(&text) {
        let m = &text[b..e];
        if m == opening {
            depth += 1;
        } else if m == closing {
            depth -= 1;
            if depth < 0 {
                return true;
            }
        }
    }
    depth != 0
}

fn allowed_slash_literal(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let style = style(cx);
    (style == "slashes" && !contains_disallowed_slash(cx, node))
        || (style == "mixed" && node.is_single_line() && !contains_disallowed_slash(cx, node))
}

fn allowed_percent_r_literal(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let style = style(cx);
    (style == "slashes" && contains_disallowed_slash(cx, node))
        || style == "percent_r"
        || (style == "mixed" && node.is_multiline())
        || contains_disallowed_slash(cx, node)
        || allowed_omit_parentheses_with_percent_r_literal(cx, node)
}

fn contains_disallowed_slash(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !cx.cop_config_bool("AllowInnerSlashes", false) && node_body(node, false).contains('/')
}

fn node_body(node: Node<'_>, include_begin_nodes: bool) -> String {
    node.child_nodes()
        .filter(|c| c.is(NodeType::Str) || (include_begin_nodes && c.is(NodeType::Begin)))
        .map(|c| c.source())
        .collect()
}

fn slash_literal(node: Node<'_>) -> bool {
    node.loc_is(node.loc().begin, "/")
}

fn preferred_delimiters(cx: &Investigation<'_, '_>) -> (String, String) {
    let delimiters = cx
        .config
        .for_cop("Style/PercentLiteralDelimiters")
        .get("PreferredDelimiters")
        .and_then(|d| d.get("%r"))
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let mut chars = delimiters.chars();
    let open = chars.next().map(String::from).unwrap_or_default();
    let close = chars.next().map(String::from).unwrap_or_default();
    (open, close)
}

fn allowed_omit_parentheses_with_percent_r_literal(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !node.parent().is_some_and(|p| p.is_call()) {
        return false;
    }
    let content: String = node.child_nodes().filter_map(|c| c.str_content()).collect();
    if content.starts_with([' ', '=']) {
        return true;
    }
    cx.config
        .for_cop("Style/MethodCallWithArgsParentheses")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("omit_parentheses")
}

fn inner_slash_indices(node: Node<'_>, pattern: &str) -> Vec<usize> {
    let text = node_body(node, true);
    let mut indices = Vec::new();
    let mut from = 0;
    while let Some(i) = text.get(from..).and_then(|t| t.find(pattern)) {
        indices.push(from + i);
        from += i + 1;
    }
    indices
}

fn inner_slash_for(opening_delimiter: &str) -> &'static str {
    if matches!(opening_delimiter, "/" | "%r/") {
        "\\/"
    } else {
        "/"
    }
}

fn calculate_replacement(cx: &Investigation<'_, '_>, node: Node<'_>) -> (String, String) {
    if slash_literal(node) {
        let (open, close) = preferred_delimiters(cx);
        (format!("%r{open}"), close)
    } else {
        ("/".to_owned(), "/".to_owned())
    }
}
