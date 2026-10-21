//! Port of `lib/rubocop/cop/lint/syntax.rb`.
//!
//! Messages come from Prism, so they match RuboCop's `parser_prism` engine
//! (the default for `TargetRubyVersion` 3.4+) rather than the `parser` gem.

use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::target_ruby::format_version;

pub struct Syntax;

impl Cop for Syntax {
    fn name(&self) -> &'static str {
        "Lint/Syntax"
    }

    fn on_other_file(&mut self, cx: &mut Investigation<'_, '_>) {
        let processed_source = cx.processed_source;
        let ruby_version = format_version(processed_source.ruby_version());
        for diagnostic in processed_source.diagnostics() {
            let message = format!(
                "{}\n(Using Ruby {ruby_version} parser; configure using `TargetRubyVersion` parameter, under `AllCops`)",
                diagnostic.message
            );
            let range = diagnostic_location(diagnostic.range, processed_source.buffer_source());
            cx.add_offense(range, &message);
        }
    }
}

/// Expand zero-length diagnostic ranges so that editors and formatters can
/// display them.
fn diagnostic_location(range: Range, source: &str) -> Range {
    if !range.is_empty() {
        range
    } else if let Some(next) = source[range.end_pos..].chars().next() {
        Range::new(range.begin_pos, range.begin_pos + next.len_utf8())
    } else if let Some(previous) = source[..range.begin_pos].chars().next_back() {
        Range::new(range.begin_pos - previous.len_utf8(), range.end_pos)
    } else {
        range
    }
}
