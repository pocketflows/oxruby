//! Port of `lib/rubocop/cop/style/redundant_regexp_argument.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::string_literals_help::enforce_double_quotes;
use crate::cop::{Cop, Investigation};
use crate::regex;

const STR_SPECIAL_CHARS: &[&str] = &[
    "\\a", "\\c", "\\C", "\\e", "\\f", "\\M", "\\n", "\\\"", "\\'", "\\\\", "\\t", "\\b",
    "\\f", "\\r", "\\u", "\\v", "\\x", "\\0", "\\1", "\\2", "\\3", "\\4", "\\5", "\\6", "\\7",
];

#[derive(Default)]
pub struct RedundantRegexpArgument;

impl Cop for RedundantRegexpArgument {
    fn name(&self) -> &'static str {
        "Style/RedundantRegexpArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "byteindex",
            "byterindex",
            "gsub",
            "gsub!",
            "partition",
            "rpartition",
            "scan",
            "split",
            "start_with?",
            "sub",
            "sub!",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(regexp_node) = node.first_argument().filter(|a| a.is(NodeType::Regexp)) else {
            return;
        };
        let regopt_empty = regexp_node
            .child_nodes()
            .last()
            .is_some_and(|r| r.is(NodeType::Regopt) && r.children().is_empty());
        let content = regexp_content(regexp_node);
        if !regopt_empty || content == " " {
            return;
        }
        // `DETERMINISTIC_REGEX = /\A(?:#{LITERAL_REGEX})+\Z/`
        if regex!(r#"\A(?:[\w\s\-,"'!#%&<>=;:`~/]|\\[^AbBdDgGhHkpPRwWXsSzZ0-9])+\Z"#)
            .find(regexp_node.source())
            .is_none()
        {
            return;
        }
        let prefer = preferred_argument(cx, &content);
        let message = format!(
            "Use string `{prefer}` as argument instead of regexp `{}`.",
            regexp_node.source()
        );
        cx.add_offense_with_correction(regexp_node.source_range(), &message, |corrector| {
            corrector.replace(regexp_node.source_range(), &prefer);
        });
    }
}

/// `RegexpNode#content`.
fn regexp_content(node: Node<'_>) -> String {
    node.child_nodes()
        .filter_map(|c| c.str_content())
        .collect()
}

fn preferred_argument(cx: &Investigation<'_, '_>, content: &str) -> String {
    let mut new_argument = replacement(content);
    let quote;
    if new_argument.contains('"') {
        new_argument = new_argument.replace('\'', "\\'").replace("\\\"", "\"");
        quote = "'";
    } else if new_argument.contains("\\'") {
        new_argument = regex!(r"(?<!\\)((?:\\\\)*)'")
            .replace_all(&new_argument, |caps: &onig::Captures<'_>| {
                format!("{}\\'", caps.at(1).unwrap_or(""))
            });
        quote = "'";
    } else if new_argument.contains('\'') {
        new_argument = new_argument.replace('\'', "\\'");
        quote = "'";
    } else if new_argument.contains('\\') {
        quote = "\"";
    } else {
        quote = if enforce_double_quotes(cx) { "\"" } else { "'" };
    }
    format!("{quote}{new_argument}{quote}")
}

fn replacement(content: &str) -> String {
    let mut stack: Option<char> = None;
    let mut chars: Vec<String> = Vec::new();
    for c in content.chars() {
        if stack.is_none() && c == '\\' {
            stack = Some(c);
        } else {
            let mut s = String::new();
            if let Some(prev) = stack.take() {
                s.push(prev);
            }
            s.push(c);
            chars.push(s);
        }
    }
    chars
        .into_iter()
        .map(|c| {
            if STR_SPECIAL_CHARS.contains(&c.as_str()) {
                c
            } else {
                c.replace('\\', "")
            }
        })
        .collect()
}
