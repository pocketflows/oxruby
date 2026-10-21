//! Port of `lib/rubocop/cop/style/redundant_line_continuation.rb`.

use crate::cop::mixin::match_range::each_match_range;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::reparsed_equivalence::{
    Oversized, ReparsedEquivalence, verified_by_reparse,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

const MSG: &str = "Redundant line continuation.";
const STRING_TOKEN_TYPES: &[&str] = &["tSTRING", "tSTRING_CONTENT"];
const STRING_LITERAL_ENDING_TOKEN_TYPES: &[&str] = &["tSTRING", "tSTRING_END"];
const STRING_LITERAL_BEGINNING_TOKEN_TYPES: &[&str] = &["tSTRING", "tSTRING_BEG"];

#[derive(Default)]
pub struct RedundantLineContinuation;

impl Cop for RedundantLineContinuation {
    fn name(&self) -> &'static str {
        "Style/RedundantLineContinuation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(ast) = ps.ast() else {
            return;
        };
        let candidates = line_continuation_candidates(ps, ast.source_range());
        for range in verified_by_reparse(&*self, cx, &candidates, Oversized::Verify) {
            cx.add_offense_with_correction(range, MSG, |corrector| {
                corrector.remove_leading(range, 1);
            });
        }
        // inspect_end_of_ruby_code_line_continuation
        let last_line_number = ast.last_line();
        let Some(last_line) = ps.lines().get(last_line_number - 1) else {
            return;
        };
        if !last_line.ends_with('\\') {
            return;
        }
        let line_range = ps.line_range(last_line_number);
        let range = range_between(line_range.end_pos - 1, line_range.end_pos);
        if within_comment(ps, range)
            || verified_by_reparse(&*self, cx, &[range], Oversized::Verify)
                .is_empty()
        {
            return;
        }
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove_trailing(range, 1);
        });
    }
}

impl ReparsedEquivalence<Range> for RedundantLineContinuation {
    fn apply_reparse_correction(
        &self,
        _cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        range: Range,
    ) {
        corrector.remove_leading(range, 1);
    }

    fn item_range(&self, item: Range) -> Range {
        item
    }
}

/// `line_continuation_candidates`.
fn line_continuation_candidates(ps: &ProcessedSource<'_>, range: Range) -> Vec<Range> {
    let source = &ps.buffer_source()[range.begin_pos..range.end_pos];
    let string_content_ranges: Vec<Range> = ps
        .tokens()
        .iter()
        .filter(|t| STRING_TOKEN_TYPES.contains(&t.ty))
        .map(|t| t.range)
        .collect();
    each_match_range(range, source, regex!(r"(\\\n)"))
        .into_iter()
        .filter(|&r| {
            !within_comment(ps, r)
                && !implicit_string_concatenation(ps, r)
                && !string_content_ranges.iter().any(|pos| pos.overlaps(r))
                && !leading_dot_method_chain_with_blank_line(ps, r)
        })
        .collect()
}

fn within_comment(ps: &ProcessedSource<'_>, range: Range) -> bool {
    ps.comments().iter().any(|c| c.range.overlaps(range))
}

fn implicit_string_concatenation(ps: &ProcessedSource<'_>, range: Range) -> bool {
    let tokens = ps.sorted_tokens();
    let index = tokens.partition_point(|t| t.range.end_pos <= range.begin_pos);
    let line = ps.line_of(range.begin_pos);
    // `bsearch_index { |token| token.end_pos > range.begin_pos }`, then the
    // token before it (nil when there is none, or it's the first).
    if index == 0 || index == tokens.len() {
        return false;
    }
    let before = &tokens[index - 1];
    if !(STRING_LITERAL_ENDING_TOKEN_TYPES.contains(&before.ty)
        && ps.line_of(before.range.end_pos) == line)
    {
        return false;
    }
    let after_index = tokens.partition_point(|t| t.range.begin_pos < range.end_pos);
    tokens.get(after_index).is_some_and(|after| {
        STRING_LITERAL_BEGINNING_TOKEN_TYPES.contains(&after.ty)
            && ps.line_of(after.range.begin_pos) == line + 1
    })
}

fn leading_dot_method_chain_with_blank_line(ps: &ProcessedSource<'_>, range: Range) -> bool {
    let line = ps.line_of(range.begin_pos);
    let source_line = ps.source_line(line).trim();
    if !(source_line.starts_with('.') || source_line.starts_with("&.")) {
        return false;
    }
    ps.lines().get(line).is_some_and(|next| next.trim().is_empty())
}
