//! Port of `lib/rubocop/formatter/`. Formatters receive the same callbacks as
//! RuboCop's `BaseFormatter`.

mod clang_style_formatter;
mod colorizable;
mod emacs_style_formatter;
mod json_formatter;
mod progress_formatter;
mod simple_text_formatter;
mod text_util;

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cop::Offense;

pub use colorizable::Colorizable;

pub trait Formatter {
    fn started(&mut self, _target_files: &[PathBuf]) {}
    fn file_started(&mut self, _file: &Path) {}
    fn file_finished(&mut self, file: &Path, offenses: &[Offense]);
    fn finished(&mut self, inspected_files: &[PathBuf]);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FormatterOptions {
    pub color: bool,
    /// `-a` (as opposed to `-A`) was given; changes the summary wording.
    pub safe_autocorrect: bool,
}

/// Port of `FormatterSet::BUILTIN_FORMATTERS_FOR_KEYS` (the ported subset).
pub fn build(
    key: &str,
    output: Box<dyn Write>,
    options: FormatterOptions,
) -> Result<Box<dyn Formatter>, String> {
    let color = Colorizable {
        enabled: options.color,
    };
    Ok(match key {
        "progress" | "p" => Box::new(progress_formatter::ProgressFormatter::new(
            output, color, options,
        )),
        "simple" | "s" => Box::new(simple_text_formatter::SimpleTextFormatter::new(
            output, color, options, false,
        )),
        "quiet" | "q" => Box::new(simple_text_formatter::SimpleTextFormatter::new(
            output, color, options, true,
        )),
        "clang" | "c" => Box::new(clang_style_formatter::ClangStyleFormatter::new(
            output, color, options,
        )),
        "emacs" | "e" => Box::new(emacs_style_formatter::EmacsStyleFormatter::new(
            output, color,
        )),
        "json" | "j" => Box::new(json_formatter::JsonFormatter::new(output)),
        _ => return Err(format!("Formatter \"{key}\" not found")),
    })
}

/// Formats whose output must stay machine readable (no corrected source is
/// echoed after them with `--stdin`).
pub const INTEGRATION_FORMATTERS: &[&str] = &["json", "j", "junit", "html", "h"];
