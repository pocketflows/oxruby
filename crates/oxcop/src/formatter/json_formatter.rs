//! Port of `lib/rubocop/formatter/json_formatter.rb`.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::Formatter;
use crate::cop::Offense;
use crate::path_util::smart_path;
use crate::version::RUBOCOP_VERSION;

pub struct JsonFormatter {
    output: Box<dyn Write>,
    files: Vec<Value>,
    offense_count: usize,
    target_file_count: usize,
}

impl JsonFormatter {
    pub fn new(output: Box<dyn Write>) -> Self {
        Self {
            output,
            files: Vec::new(),
            offense_count: 0,
            target_file_count: 0,
        }
    }

    /// The Ruby fields are filled in by the gem's launcher, which knows which
    /// Ruby it runs under; the binary on its own does not.
    fn metadata_hash() -> Value {
        let env = |key: &str| std::env::var(key).unwrap_or_default();
        json!({
            "rubocop_version": RUBOCOP_VERSION,
            "ruby_engine": env("OXCOP_RUBY_ENGINE"),
            "ruby_version": env("OXCOP_RUBY_VERSION"),
            "ruby_patchlevel": env("OXCOP_RUBY_PATCHLEVEL"),
            "ruby_platform": env("OXCOP_RUBY_PLATFORM"),
        })
    }

    fn hash_for_offense(offense: &Offense) -> Value {
        json!({
            "severity": offense.severity.name(),
            "message": offense.message,
            "cop_name": offense.cop_name,
            "corrected": offense.corrected(),
            "correctable": offense.correctable(),
            "location": {
                "start_line": offense.line(),
                "start_column": offense.real_column(),
                "last_line": offense.last_line(),
                "last_column": offense.real_last_column(),
                "length": offense.location.length,
                "line": offense.line(),
                "column": offense.real_column(),
            },
        })
    }
}

impl Formatter for JsonFormatter {
    fn started(&mut self, target_files: &[PathBuf]) {
        self.target_file_count = target_files.len();
    }

    fn file_finished(&mut self, file: &Path, offenses: &[Offense]) {
        self.files.push(json!({
            "path": smart_path(file),
            "offenses": offenses.iter().map(Self::hash_for_offense).collect::<Vec<_>>(),
        }));
        self.offense_count += offenses.len();
    }

    fn finished(&mut self, inspected_files: &[PathBuf]) {
        let hash = json!({
            "metadata": Self::metadata_hash(),
            "files": std::mem::take(&mut self.files),
            "summary": {
                "offense_count": self.offense_count,
                "target_file_count": self.target_file_count,
                "inspected_file_count": inspected_files.len(),
            },
        });
        let _ = write!(self.output, "{hash}");
        let _ = self.output.flush();
    }
}
