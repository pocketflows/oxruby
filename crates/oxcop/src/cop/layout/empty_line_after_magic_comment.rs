//! Port of `lib/rubocop/cop/layout/empty_line_after_magic_comment.rb`.

use serde_yaml_ng::Value;

use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};
use crate::magic_comment::MagicComment;
use crate::processed_source::{Comment, ProcessedSource};

#[derive(Default)]
pub struct EmptyLineAfterMagicComment;

impl Cop for EmptyLineAfterMagicComment {
    fn name(&self) -> &'static str {
        "Layout/EmptyLineAfterMagicComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn validate_config(&self, cx: &Investigation<'_, '_>) -> Result<(), String> {
        let value = cx.cop_config_value("NumberOfEmptyLines");
        if value.is_none_or(|v| v.as_i64().is_some_and(|n| n > 0)) {
            return Ok(());
        }
        let inspected = match value {
            Some(Value::String(s)) => crate::cop::mixin::util::ruby_inspect(s),
            Some(Value::Null) | None => "nil".to_owned(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            Some(other) => serde_yaml_ng::to_string(other)
                .unwrap_or_default()
                .trim_end()
                .to_owned(),
        };
        Err(format!(
            "The `Layout/EmptyLineAfterMagicComment` cop only accepts a positive integer for \
             its `NumberOfEmptyLines` configuration parameter, but `{inspected}` was given."
        ))
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(last_magic_comment) = last_magic_comment(ps) else {
            return;
        };
        let number_of_empty_lines = number_of_empty_lines(cx);
        let line = ps.line_of(last_magic_comment.range.begin_pos);
        let actual = empty_lines_after(ps, line);
        // Trailing empty lines are `Layout/TrailingEmptyLines`' responsibility.
        if ps.lines().get(line + actual).is_none() {
            return;
        }
        if actual >= number_of_empty_lines {
            return;
        }
        let offending_range = source_range(ps, line + 1, 0, 1);
        let lines = if number_of_empty_lines == 1 { "line" } else { "lines" };
        let message = format!(
            "Expected at least {number_of_empty_lines} empty {lines} after magic comments; found {actual}."
        );
        cx.add_offense_with_correction(offending_range, &message, |corrector| {
            corrector.insert_before(offending_range, "\n".repeat(number_of_empty_lines - actual));
        });
    }
}

fn number_of_empty_lines(cx: &Investigation<'_, '_>) -> usize {
    match cx.cop_config_value("NumberOfEmptyLines") {
        None => 1,
        Some(Value::Number(n)) => n.as_u64().unwrap_or(1) as usize,
        Some(_) => 1,
    }
}

fn empty_lines_after(ps: &ProcessedSource<'_>, first_line: usize) -> usize {
    let lines = ps.lines();
    let mut count = 0;
    while lines
        .get(first_line + count)
        .is_some_and(|l| l.trim().is_empty())
    {
        count += 1;
    }
    count
}

fn last_magic_comment<'c, 's>(ps: &'c ProcessedSource<'s>) -> Option<&'c Comment<'s>> {
    let ast_line = ps.ast().map(|ast| ast.first_line());
    ps.comments()
        .iter()
        .take_while(|c| ast_line.is_none_or(|line| ps.line_of(c.range.begin_pos) < line))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find(|c| MagicComment::parse(c.text).any())
}
