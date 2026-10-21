//! Port of `lib/rubocop/cop/layout/trailing_empty_lines.rb`.

use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct TrailingEmptyLines;

impl Cop for TrailingEmptyLines {
    fn name(&self) -> &'static str {
        "Layout/TrailingEmptyLines"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let source = cx.processed_source.buffer_source();
        if source.is_empty() {
            return;
        }
        if ends_in_end(cx) {
            return;
        }
        if end_with_percent_blank_string(source) {
            return;
        }
        let whitespace_at_end = regex!(r"\s*\Z")
            .find(source)
            .map_or("", |(begin, end)| &source[begin..end]);
        let blank_lines = whitespace_at_end.matches('\n').count() as isize - 1;
        let wanted_blank_lines = if style(cx) == "final_newline" { 0 } else { 1 };
        if blank_lines == wanted_blank_lines {
            return;
        }
        offense_detected(cx, wanted_blank_lines, blank_lines, whitespace_at_end);
    }
}

fn offense_detected(
    cx: &mut Investigation<'_, '_>,
    wanted_blank_lines: isize,
    blank_lines: isize,
    whitespace_at_end: &str,
) {
    let source_len = cx.processed_source.buffer_source().len();
    let mut begin_pos = source_len - whitespace_at_end.len();
    let autocorrect_range = range_between(begin_pos, source_len);
    if !whitespace_at_end.is_empty() {
        begin_pos += 1;
    }
    let report_range = range_between(begin_pos, source_len);
    let replacement = if style(cx) == "final_newline" {
        "\n"
    } else {
        "\n\n"
    };
    cx.add_offense_with_correction(
        report_range,
        &message(wanted_blank_lines, blank_lines),
        |corrector| corrector.replace(autocorrect_range, replacement),
    );
}

fn ends_in_end(cx: &Investigation<'_, '_>) -> bool {
    let source = cx.processed_source.buffer_source();
    if source.contains("__END__") {
        return true;
    }
    let tokens = cx.processed_source.tokens();
    let Some(last) = tokens.last() else {
        return false;
    };
    source
        .get(last.range.end_pos..)
        .is_some_and(|extra| extra.trim().starts_with("__END__"))
}

fn end_with_percent_blank_string(source: &str) -> bool {
    source.ends_with("%\n\n")
}

fn message(wanted_blank_lines: isize, blank_lines: isize) -> String {
    match blank_lines {
        -1 => "Final newline missing.".to_owned(),
        0 => "Trailing blank line missing.".to_owned(),
        _ => {
            let instead_of = if wanted_blank_lines == 0 {
                String::new()
            } else {
                format!("instead of {wanted_blank_lines} ")
            };
            format!("{blank_lines} trailing blank lines {instead_of}detected.")
        }
    }
}
