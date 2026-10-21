//! Port of `lib/rubocop/cop/mixin/code_length.rb`.
//!
//! `exclude_limit 'Max'` (`self.max = length`) only feeds
//! `--auto-gen-config` and is not ported; `LSP.enabled?` is false.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::metrics::utils::code_length_calculator::{CodeLengthCalculator, Foldable};

/// `message(length, max_length)` with `MSG` and the cop's label.
pub fn message(label: &str, length: usize, max_length: i64) -> String {
    format!("{label} has too many lines. [{length}/{max_length}]")
}

pub fn max_length(cx: &Investigation<'_, '_>) -> i64 {
    cx.cop_config_int("Max").unwrap_or(0)
}

pub fn count_comments(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("CountComments", false)
}

/// `count_as_one`, as foldable types. Unknown types are an error upstream
/// (`raise Warning, "Unknown foldable type: ..."`); they are returned as
/// `Err` with that message.
pub fn count_as_one(cx: &Investigation<'_, '_>) -> Result<Vec<Foldable>, String> {
    cx.cop_config_list("CountAsOne")
        .into_iter()
        .map(|name| {
            Foldable::from_name(name).ok_or_else(|| {
                format!(
                    "Unknown foldable type: :{name}. Valid foldable types are: \
                     array, hash, heredoc, method_call."
                )
            })
        })
        .collect()
}

/// `check_code_length(node)`, reporting with `label` (`cop_label`).
pub fn check_code_length<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, label: &str) {
    let max_length = max_length(cx);
    // Skip costly calculation when definitely not needed
    if node.line_count() as i64 <= max_length {
        return;
    }
    let foldable_types = match count_as_one(cx) {
        Ok(types) => types,
        Err(message) => {
            cx.errors.push(message);
            return;
        }
    };
    let calculator = CodeLengthCalculator::new(
        node,
        cx.processed_source,
        count_comments(cx),
        foldable_types,
    );
    let length = calculator.calculate();
    if length as i64 <= max_length {
        return;
    }
    let location = location(node);
    cx.add_offense(location, &message(label, length, max_length));
}

fn location(node: Node<'_>) -> crate::processed_source::Range {
    if node.is(NodeType::Casgn) {
        return node.loc().name.unwrap();
    }
    node.source_range()
}
