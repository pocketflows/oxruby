//! Port of `lib/rubocop/cop/style/command_literal.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Corrector, Cop, Investigation};

const MSG_USE_BACKTICKS: &str = "Use backticks around command string.";
const MSG_USE_PERCENT_X: &str = "Use `%x` around command string.";

#[derive(Default)]
pub struct CommandLiteral;

impl Cop for CommandLiteral {
    fn name(&self) -> &'static str {
        "Style/CommandLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Xstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_heredoc() {
            return;
        }
        let style = style(cx);
        let allow_inner = cx.cop_config_bool("AllowInnerBackticks", false);
        let disallowed_backtick = !allow_inner && contains_backtick(node);
        let (allowed, message) = if backtick_literal(node) {
            let allowed = match style {
                "backticks" => !disallowed_backtick,
                "mixed" => node.is_single_line() && !disallowed_backtick,
                _ => false,
            };
            (allowed, MSG_USE_PERCENT_X)
        } else {
            let allowed = match style {
                "backticks" => disallowed_backtick,
                "mixed" => node.is_multiline() || disallowed_backtick,
                "percent_x" => true,
                _ => false,
            };
            (allowed, MSG_USE_BACKTICKS)
        };
        if allowed {
            return;
        }
        let delimiter = preferred_delimiter(cx);
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            autocorrect(corrector, node, &delimiter);
        });
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, delimiter: &[char]) {
    if contains_backtick(node) {
        return;
    }
    let (first, last) = if backtick_literal(node) {
        (
            format!("%x{}", delimiter.first().map_or(String::new(), char::to_string)),
            delimiter.get(1).map_or(String::new(), char::to_string),
        )
    } else {
        ("`".to_owned(), "`".to_owned())
    };
    corrector.replace(node.loc().begin.unwrap(), first);
    corrector.replace(node.loc().end.unwrap(), last);
}

fn contains_backtick(node: Node<'_>) -> bool {
    node_body(node).contains('`')
}

fn node_body(node: Node<'_>) -> &str {
    let begin = node.loc().begin.unwrap();
    let end = node.loc().end.unwrap();
    let source = node.source();
    source
        .get(begin.len()..source.len().saturating_sub(end.len()))
        .unwrap_or("")
}

fn backtick_literal(node: Node<'_>) -> bool {
    node.loc_is(node.loc().begin, "`")
}

fn preferred_delimiter(cx: &Investigation<'_, '_>) -> Vec<char> {
    let config = cx
        .config
        .for_cop("Style/PercentLiteralDelimiters")
        .get("PreferredDelimiters");
    let lookup = |key: &str| {
        config
            .and_then(|c| c.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    lookup("%x")
        .or_else(|| lookup("default"))
        .unwrap_or_default()
        .chars()
        .collect()
}
