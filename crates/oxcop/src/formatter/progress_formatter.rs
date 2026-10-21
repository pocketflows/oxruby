//! Port of `lib/rubocop/formatter/progress_formatter.rb`, RuboCop's default.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::clang_style_formatter::report_file;
use super::simple_text_formatter::{Stats, summary};
use super::text_util::pluralize;
use super::{Colorizable, Formatter, FormatterOptions};
use crate::cop::Offense;

pub struct ProgressFormatter {
    output: Box<dyn Write>,
    color: Colorizable,
    options: FormatterOptions,
    stats: Stats,
    offenses_for_files: Vec<(PathBuf, Vec<Offense>)>,
}

impl ProgressFormatter {
    pub fn new(output: Box<dyn Write>, color: Colorizable, options: FormatterOptions) -> Self {
        Self {
            output,
            color,
            options,
            stats: Stats::default(),
            offenses_for_files: Vec::new(),
        }
    }
}

impl Formatter for ProgressFormatter {
    fn started(&mut self, target_files: &[PathBuf]) {
        let _ = writeln!(
            self.output,
            "Inspecting {}",
            pluralize(target_files.len(), "file", false)
        );
    }

    fn file_finished(&mut self, file: &Path, offenses: &[Offense]) {
        let mark = match offenses.iter().map(|o| o.severity).max() {
            None => self.color.green("."),
            Some(severity) => self.color.severity_code(severity),
        };
        if !offenses.is_empty() {
            self.stats.count(offenses);
            self.offenses_for_files
                .push((file.to_path_buf(), offenses.to_vec()));
        }
        let _ = write!(self.output, "{mark}");
        let _ = self.output.flush();
    }

    fn finished(&mut self, inspected_files: &[PathBuf]) {
        let _ = writeln!(self.output);
        if !self.offenses_for_files.is_empty() {
            let _ = write!(self.output, "\nOffenses:\n\n");
            for (file, offenses) in &self.offenses_for_files {
                report_file(&mut *self.output, &self.color, file, offenses);
            }
        }
        let summary = summary(&self.color, self.options, inspected_files.len(), self.stats);
        let _ = write!(self.output, "\n{summary}\n");
        let _ = self.output.flush();
    }
}
