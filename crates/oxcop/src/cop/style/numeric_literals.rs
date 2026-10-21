//! Port of `lib/rubocop/cop/style/numeric_literals.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_pattern::allowed_patterns;
use crate::cop::mixin::integer_node::integer_part;
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Use underscores(_) as thousands separator and separate every 3 digits with them.";

#[derive(Default)]
pub struct NumericLiterals;

impl Cop for NumericLiterals {
    fn name(&self) -> &'static str {
        "Style/NumericLiterals"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Int, NodeType::Float]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let int = integer_part(node);
        if int.starts_with('0') {
            return;
        }
        if allowed_numbers(cx).iter().any(|n| n == int) {
            return;
        }
        if allowed_patterns(cx)
            .iter()
            .filter_map(|p| ruby_regex(&format!(r"\A(?:{p})\z")))
            .any(|re| re.find(int).is_some())
        {
            return;
        }
        let min_digits = cx.cop_config_int("MinDigits").unwrap_or(0);
        if (int.chars().count() as i64) < min_digits {
            return;
        }
        let short_group = if cx.cop_config_bool("Strict", false) {
            regex!(r"_\d{1,2}(_|$)")
        } else {
            regex!(r"_\d{1,2}_")
        };
        let only_digits = regex!(r"^\d+$").find(int).is_some();
        if !(only_digits || regex!(r"\d{4}").find(int).is_some() || short_group.find(int).is_some())
        {
            return;
        }
        let range = node.source_range();
        // The `add_offense` block (the auto-gen-config bookkeeping and the
        // correction) runs for a new offense on an enabled line.
        if !cx.has_offense_at(range) && cx.enabled_line(cx.processed_source.line_of(range.begin_pos)) {
            if only_digits {
                cx.set_exclude_limit("MinDigits", int.chars().count() + 1);
            } else {
                let config = cx.config_to_allow_offenses();
                config.clear();
                config.insert("Enabled".into(), Value::Bool(false));
            }
        }
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(range, format_number(node));
        });
    }
}

fn allowed_numbers(cx: &Investigation<'_, '_>) -> Vec<String> {
    match cx.cop_config_value("AllowedNumbers") {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn format_number(node: Node<'_>) -> String {
    let source: String = node.source().split_whitespace().collect();
    match source.find(['e', 'E', '.']) {
        Some(i) => format!("{}{}", format_int_part(&source[..i]), &source[i..]),
        None => format_int_part(&source),
    }
}

/// `Integer(int_part)` formatted with `_` every three digits.
fn format_int_part(int_part: &str) -> String {
    let negative = int_part.starts_with('-');
    let digits: String = int_part
        .trim_start_matches(['+', '-'])
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('_');
        }
        out.push(c);
    }
    if negative && out != "0" {
        out.insert(0, '-');
    }
    out
}
