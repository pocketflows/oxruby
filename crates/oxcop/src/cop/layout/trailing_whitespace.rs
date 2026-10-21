//! Port of `lib/rubocop/cop/layout/trailing_whitespace.rb`.

use ruby_prism::Visit;

use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Trailing whitespace detected.";

pub struct TrailingWhitespace;

impl Cop for TrailingWhitespace {
    fn name(&self) -> &'static str {
        "Layout/TrailingWhitespace"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let processed_source = cx.processed_source;
        let heredocs = extract_heredocs(processed_source);
        for (index, line) in processed_source.lines().iter().enumerate() {
            if !line.chars().next_back().is_some_and(is_blank) {
                continue;
            }
            process_line(cx, &heredocs, line, index + 1);
        }
    }
}

/// Ruby's `[[:blank:]]`: tab plus the Unicode space separators (`Zs`).
fn is_blank(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

struct Heredoc {
    /// Body lines, `first_line...last_line` (the terminator line excluded).
    lines: std::ops::Range<usize>,
    body: Range,
    /// `<<~'EOS'` and friends do not interpolate.
    static_: bool,
}

fn process_line(cx: &mut Investigation<'_, '_>, heredocs: &[Heredoc], line: &str, lineno: usize) {
    let heredoc = heredocs
        .iter()
        .find(|heredoc| heredoc.lines.contains(&lineno));
    if cx.cop_config_bool("AllowInHeredoc", false) && heredoc.is_some() {
        return;
    }

    let range = offense_range(cx.processed_source, lineno, line);
    let processed_source = cx.processed_source;
    cx.add_offense_with_correction(range, MSG, |corrector| match heredoc {
        Some(heredoc) => process_line_in_heredoc(processed_source, corrector, range, heredoc),
        None => corrector.remove(range),
    });
}

fn process_line_in_heredoc(
    processed_source: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    range: Range,
    heredoc: &Heredoc,
) {
    let indent_level = indent_level(processed_source.slice(heredoc.body));
    let whitespace_only = whitespace_only(processed_source.buffer_source(), range);
    if whitespace_only && processed_source.slice(range).chars().count() <= indent_level {
        corrector.remove(range);
    } else if !heredoc.static_ {
        let range = if whitespace_only {
            let skipped: usize = processed_source
                .slice(range)
                .chars()
                .take(indent_level)
                .map(char::len_utf8)
                .sum();
            Range::new(range.begin_pos + skipped, range.end_pos)
        } else {
            range
        };
        corrector.wrap(range, "#{'", "'}");
    }
}

/// `range_with_surrounding_space(range).source` starts and ends with a
/// newline, i.e. the line holds nothing but this whitespace.
fn whitespace_only(source: &str, range: Range) -> bool {
    let bytes = source.as_bytes();
    let mut begin = range.begin_pos;
    while begin > 0 && matches!(bytes[begin - 1], b' ' | b'\t') {
        begin -= 1;
    }
    let mut end = range.end_pos;
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    begin > 0 && bytes[begin - 1] == b'\n' && end < bytes.len() && bytes[end] == b'\n'
}

/// Port of `Heredoc#indent_level`.
fn indent_level(body: &str) -> usize {
    body.split_inclusive('\n')
        .filter_map(|line| {
            let rest = line.trim_start_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']);
            let indentation = &line[..line.len() - rest.len()];
            (!indentation.ends_with('\n')).then(|| indentation.chars().count())
        })
        .min()
        .unwrap_or(0)
}

fn offense_range(processed_source: &ProcessedSource<'_>, lineno: usize, line: &str) -> Range {
    let line_range = processed_source.line_range(lineno);
    let trimmed = line.trim_end_matches(is_blank).len();
    Range::new(
        line_range.begin_pos + trimmed,
        line_range.begin_pos + line.len(),
    )
}

fn extract_heredocs(processed_source: &ProcessedSource<'_>) -> Vec<Heredoc> {
    let mut visitor = HeredocVisitor {
        processed_source,
        heredocs: Vec::new(),
    };
    visitor.visit(&processed_source.prism_ast());
    visitor.heredocs
}

struct HeredocVisitor<'a, 'src> {
    processed_source: &'a ProcessedSource<'src>,
    heredocs: Vec<Heredoc>,
}

impl HeredocVisitor<'_, '_> {
    fn record(
        &mut self,
        opening: Option<ruby_prism::Location<'_>>,
        body_start: Option<usize>,
        closing: Option<ruby_prism::Location<'_>>,
    ) {
        let (Some(opening), Some(closing)) = (opening, closing) else {
            return;
        };
        let opening = opening.as_slice();
        if !opening.starts_with(b"<<") {
            return;
        }
        let body_end = closing.start_offset();
        let body_start = body_start.unwrap_or(body_end).min(body_end);
        self.heredocs.push(Heredoc {
            lines: self.processed_source.line_of(body_start)
                ..self.processed_source.line_of(body_end),
            body: Range::new(body_start, body_end),
            static_: opening.ends_with(b"'"),
        });
    }
}

impl<'pr> Visit<'pr> for HeredocVisitor<'_, '_> {
    fn visit_string_node(&mut self, node: &ruby_prism::StringNode<'pr>) {
        self.record(
            node.opening_loc(),
            Some(node.content_loc().start_offset()),
            node.closing_loc(),
        );
        ruby_prism::visit_string_node(self, node);
    }

    fn visit_x_string_node(&mut self, node: &ruby_prism::XStringNode<'pr>) {
        self.record(
            Some(node.opening_loc()),
            Some(node.content_loc().start_offset()),
            Some(node.closing_loc()),
        );
        ruby_prism::visit_x_string_node(self, node);
    }

    fn visit_interpolated_string_node(&mut self, node: &ruby_prism::InterpolatedStringNode<'pr>) {
        let body_start = node
            .parts()
            .first()
            .map(|part| part.location().start_offset());
        self.record(node.opening_loc(), body_start, node.closing_loc());
        ruby_prism::visit_interpolated_string_node(self, node);
    }

    fn visit_interpolated_x_string_node(
        &mut self,
        node: &ruby_prism::InterpolatedXStringNode<'pr>,
    ) {
        let body_start = node
            .parts()
            .first()
            .map(|part| part.location().start_offset());
        self.record(
            Some(node.opening_loc()),
            body_start,
            Some(node.closing_loc()),
        );
        ruby_prism::visit_interpolated_x_string_node(self, node);
    }
}
