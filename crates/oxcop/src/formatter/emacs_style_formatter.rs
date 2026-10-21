//! Port of `lib/rubocop/formatter/emacs_style_formatter.rb`.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::{Colorizable, Formatter};
use crate::cop::Offense;

pub struct EmacsStyleFormatter {
    output: Box<dyn Write>,
    color: Colorizable,
}

impl EmacsStyleFormatter {
    pub fn new(output: Box<dyn Write>, color: Colorizable) -> Self {
        Self { output, color }
    }
}

impl Formatter for EmacsStyleFormatter {
    fn file_finished(&mut self, file: &Path, offenses: &[Offense]) {
        for offense in offenses {
            let prefix = if offense.corrected() {
                self.color.green("[Corrected] ")
            } else if offense.correctable() {
                self.color.yellow("[Correctable] ")
            } else {
                String::new()
            };
            let message = format!("{prefix}{}", offense.message).replace('\n', " ");
            let _ = writeln!(
                self.output,
                "{}:{}:{}: {}: {message}",
                file.display(),
                offense.line(),
                offense.real_column(),
                offense.severity.code()
            );
        }
    }

    fn finished(&mut self, _inspected_files: &[PathBuf]) {
        let _ = self.output.flush();
    }
}
