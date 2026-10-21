//! Port of `lib/rubocop/cop/layout/heredoc_indentation.rb`.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, heredoc_node, indent_level};
use crate::cop::{Cop, Corrector, Investigation};
use crate::regex;

static SQUISH_METHOD: NodePattern = NodePattern::new("(send _ {:squish :squish!})");

#[derive(Default)]
pub struct HeredocIndentation;

impl Cop for HeredocIndentation {
    fn name(&self) -> &'static str {
        "Layout/HeredocIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        HEREDOC_NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = heredoc_node(node) else {
            return;
        };
        let body = heredoc_body(cx, node);
        if body.trim().is_empty() {
            return;
        }
        let body_indent_level = indent_level(body);
        let heredoc_indent_type = heredoc_indent_type(node);
        if heredoc_indent_type == Some('~') {
            let expected = base_indent_level(cx, node) + configured_indentation_width(cx);
            if expected == body_indent_level {
                return;
            }
        } else if !(body_indent_level == 0 || heredoc_squish(cx, node)) {
            return;
        }
        if line_too_long(cx, node) {
            return;
        }
        register_offense(cx, node, heredoc_indent_type);
    }
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, heredoc_indent_type: Option<char>) {
    let message = message(cx, heredoc_indent_type);
    let squish = heredoc_squish(cx, node);
    cx.add_offense_cx(node.loc().heredoc_body.unwrap(), &message, |corrector, cx| {
        if heredoc_indent_type == Some('~') {
            adjust_squiggly(cx, corrector, node);
        } else if squish {
            adjust_squiggly(cx, corrector, node);
            adjust_minus(corrector, node);
        } else {
            adjust_minus(corrector, node);
        }
    });
}

fn message(cx: &Investigation<'_, '_>, heredoc_indent_type: Option<char>) -> String {
    let width = configured_indentation_width(cx);
    let current_indent_type = match heredoc_indent_type {
        Some(c) => format!("<<{c}"),
        None => "<<".to_owned(),
    };
    if current_indent_type == "<<~" {
        format!("Use {width} spaces for indentation in a heredoc.")
    } else {
        format!(
            "Use {width} spaces for indentation in a heredoc by using `<<~` instead of `{current_indent_type}`."
        )
    }
}

/// `max_line_length` (`AutocorrectLogic`).
fn max_line_length(cx: &Investigation<'_, '_>) -> Option<usize> {
    if !cx.config.cop_enabled_in_config("Layout/LineLength") {
        return None;
    }
    Some(
        cx.config
            .for_cop("Layout/LineLength")
            .get("Max")
            .and_then(Value::as_u64)
            .unwrap_or(120) as usize,
    )
}

fn line_too_long(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(max_line_length) = max_line_length(cx) else {
        return false;
    };
    let allow_heredoc = cx.config.for_cop("Layout/LineLength").get("AllowHeredoc");
    if !matches!(allow_heredoc, None | Some(Value::Null) | Some(Value::Bool(false))) {
        return false;
    }
    let body = heredoc_body(cx, node);
    let expected_indent = (base_indent_level(cx, node) + configured_indentation_width(cx)) as isize;
    let actual_indent = indent_level(body) as isize;
    let increase_indent_level = expected_indent - actual_indent;
    longest_line(body) as isize + increase_indent_level >= max_line_length as isize
}

/// `longest_line(lines).size`.
fn longest_line(lines: &str) -> usize {
    lines
        .split_inclusive('\n')
        .map(|line| line.trim_end_matches('\n').chars().count())
        .max()
        .unwrap_or(0)
}

fn adjust_squiggly(cx: &Investigation<'_, '_>, corrector: &mut Corrector, node: Node<'_>) {
    corrector.replace(node.loc().heredoc_body.unwrap(), indented_body(cx, node));
    corrector.replace(node.loc().heredoc_end.unwrap(), indented_end(cx, node));
}

fn adjust_minus(corrector: &mut Corrector, node: Node<'_>) {
    let heredoc_beginning = node.source();
    let corrected = regex!(r"<<-?").replace(heredoc_beginning, "<<~");
    corrector.replace(node.source_range(), corrected);
}

fn indented_body(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let body = heredoc_body(cx, node);
    let body_indent_level = indent_level(body);
    let correct_indent_level = base_indent_level(cx, node) + configured_indentation_width(cx);
    let replacement = " ".repeat(correct_indent_level);
    // `body.gsub(/^[^\S\r\n]{n}/, replacement)`
    body.split_inclusive('\n')
        .map(|line| {
            let prefix: Vec<(usize, char)> = line.char_indices().take(body_indent_level).collect();
            if prefix.len() == body_indent_level
                && prefix.iter().all(|(_, c)| c.is_whitespace() && *c != '\r' && *c != '\n')
            {
                let cut = line
                    .char_indices()
                    .nth(body_indent_level)
                    .map_or(line.len(), |(i, _)| i);
                format!("{replacement}{}", &line[cut..])
            } else {
                line.to_owned()
            }
        })
        .collect()
}

fn indented_end(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let end = cx.processed_source.slice(node.loc().heredoc_end.unwrap());
    let end_indent_level = indent_level(end);
    let correct_indent_level = base_indent_level(cx, node);
    if end_indent_level < correct_indent_level {
        let cut = end
            .char_indices()
            .nth(end_indent_level)
            .map_or(end.len(), |(i, _)| i);
        format!("{}{}", " ".repeat(correct_indent_level), &end[cut..])
    } else {
        end.to_owned()
    }
}

fn base_indent_level(cx: &Investigation<'_, '_>, node: Node<'_>) -> usize {
    indent_level(cx.processed_source.lines()[node.first_line() - 1])
}

fn heredoc_indent_type(node: Node<'_>) -> Option<char> {
    let source = node.source();
    let caps = regex!(r"^<<([~-])").captures(source)?;
    caps.at(1)?.chars().next()
}

fn heredoc_body<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> &'s str {
    cx.processed_source.slice(node.loc().heredoc_body.unwrap())
}

fn heredoc_squish(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.config.all_cops_bool("ActiveSupportExtensionsEnabled", false)
        && node.parent().is_some_and(|p| SQUISH_METHOD.is_match(p))
}
