//! Port of `lib/rubocop/cop/layout/empty_lines.rb`.

use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Extra blank line detected.";
const LINE_OFFSET: usize = 2;

#[derive(Default)]
pub struct EmptyLines;

impl Cop for EmptyLines {
    fn name(&self) -> &'static str {
        "Layout/EmptyLines"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.tokens().is_empty() || !ps.raw_source().contains("\n\n\n") {
            return;
        }
        let mut lines: Vec<usize> = ps.tokens().iter().map(|t| t.line(ps)).collect();
        lines.sort_unstable();
        lines.dedup();

        let mut ranges = Vec::new();
        let mut prev_line = 1;
        for cur_line in lines {
            if cur_line.saturating_sub(prev_line) > LINE_OFFSET {
                for line in (prev_line + 1)..cur_line {
                    if previous_and_current_lines_empty(cx, line) {
                        ranges.push(source_range(ps, line, 0, 1));
                    }
                }
            }
            prev_line = cur_line;
        }
        for range in ranges {
            cx.add_offense_with_correction(range, MSG, |corrector| corrector.remove(range));
        }
    }
}

fn previous_and_current_lines_empty(cx: &Investigation<'_, '_>, line: usize) -> bool {
    let lines = cx.processed_source.lines();
    lines.get(line - 2).is_some_and(|l| l.is_empty()) && lines.get(line - 1).is_some_and(|l| l.is_empty())
}
