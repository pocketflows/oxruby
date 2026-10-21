//! Port of `lib/rubocop/formatter/clang_style_formatter.rb`.

use std::io::Write;
use std::path::{Path, PathBuf};

use unicode_width::UnicodeWidthStr;

use super::simple_text_formatter::{Stats, message, summary};
use super::{Colorizable, Formatter, FormatterOptions};
use crate::cop::Offense;
use crate::path_util::smart_path;

const ELLIPSES: &str = "...";

pub fn report_file(output: &mut dyn Write, color: &Colorizable, file: &Path, offenses: &[Offense]) {
    for offense in offenses {
        report_offense(output, color, file, offense);
    }
}

fn report_offense(output: &mut dyn Write, color: &Colorizable, file: &Path, offense: &Offense) {
    let _ = writeln!(
        output,
        "{}:{}:{}: {}: {}",
        color.cyan(&smart_path(file)),
        offense.line(),
        offense.real_column(),
        color.severity_code(offense.severity),
        message(color, offense)
    );

    let source_line = &offense.location.source_line;
    if source_line.trim().is_empty() {
        return;
    }
    if offense.location.single_line() {
        let _ = writeln!(output, "{source_line}");
    } else {
        let _ = writeln!(output, "{source_line} {}", color.yellow(ELLIPSES));
    }
    let (space_area, source_area) = offense.highlighted_area();
    let _ = writeln!(
        output,
        "{}{}",
        to_whitespace(&space_area),
        "^".repeat(source_area.width())
    );
}

fn to_whitespace(string: &str) -> String {
    let tabs: String = string.chars().filter(|&c| c == '\t').collect();
    let rest: String = string.chars().filter(|&c| c != '\t').collect();
    format!("{tabs}{}", " ".repeat(rest.width()))
}

pub struct ClangStyleFormatter {
    output: Box<dyn Write>,
    color: Colorizable,
    options: FormatterOptions,
    stats: Stats,
}

impl ClangStyleFormatter {
    pub fn new(output: Box<dyn Write>, color: Colorizable, options: FormatterOptions) -> Self {
        Self {
            output,
            color,
            options,
            stats: Stats::default(),
        }
    }
}

impl Formatter for ClangStyleFormatter {
    fn file_finished(&mut self, file: &Path, offenses: &[Offense]) {
        if offenses.is_empty() {
            return;
        }
        self.stats.count(offenses);
        report_file(&mut *self.output, &self.color, file, offenses);
    }

    fn finished(&mut self, inspected_files: &[PathBuf]) {
        let summary = summary(&self.color, self.options, inspected_files.len(), self.stats);
        let _ = write!(self.output, "\n{summary}\n");
        let _ = self.output.flush();
    }
}
