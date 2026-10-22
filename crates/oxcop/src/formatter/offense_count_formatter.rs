//! Port of `lib/rubocop/formatter/offense_count_formatter.rb`. Not ported:
//! the progress bar it shows on a terminal, and `--display-style-guide`
//! links (oxcop has no such option).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use super::{Colorizable, Formatter};
use crate::config::Config;
use crate::cop::Offense;

/// This formatter displays the list of offended cops with a count of how
/// many offenses of their kind were found. Ordered by desc offense count
///
/// ```text
/// 26  LineLength
/// 3   OneLineConditional
/// --
/// 29  Total in 5 files
/// ```
pub struct OffenseCountFormatter {
    output: Box<dyn Write>,
    color: Colorizable,
    offense_counts: HashMap<String, usize>,
    offending_files_count: usize,
    /// `ConfigLoader.default_configuration`, which `cop_information` reads.
    default_configuration: Option<Mapping>,
}

impl OffenseCountFormatter {
    pub fn new(output: Box<dyn Write>, color: Colorizable) -> Self {
        Self {
            output,
            color,
            offense_counts: HashMap::new(),
            offending_files_count: 0,
            default_configuration: None,
        }
    }

    #[cfg(test)]
    pub fn offense_counts(&self) -> &HashMap<String, usize> {
        &self.offense_counts
    }

    pub fn report_summary(
        &mut self,
        offense_counts: &HashMap<String, usize>,
        offending_files_count: usize,
    ) {
        let per_cop_counts = ordered_offense_counts(offense_counts);
        let total_count: usize = offense_counts.values().sum();

        let mut text = String::from("\n");
        let column_width = total_count.to_string().len() + 2;
        for (cop_name, count) in per_cop_counts {
            text.push_str(&format!(
                "{:<column_width$}{}\n",
                count,
                self.cop_information(cop_name)
            ));
        }
        text.push_str("--\n");
        text.push_str(&format!(
            "{total_count}  Total in {offending_files_count} files\n"
        ));
        text.push('\n');
        let _ = self.output.write_all(text.as_bytes());
        let _ = self.output.flush();
    }

    /// The cop's name, with ` [Safe Correctable]` or ` [Unsafe Correctable]`
    /// when it supports autocorrection. As upstream, the cop is
    /// instantiated with the default configuration (`Config.new`), so
    /// `Safe`/`SafeAutoCorrect` come from there, not from the project's.
    fn cop_information(&self, cop_name: &str) -> String {
        let correctable = crate::cop::registry::all()
            .iter()
            .map(|new| new())
            .find(|cop| cop.name() == cop_name)
            .is_some_and(|cop| cop.support_autocorrect());
        if !correctable {
            return cop_name.to_owned();
        }
        let config = Config::new(self.default_configuration.clone().unwrap_or_default(), None);
        let cop_config = config.for_badge(cop_name);
        let flag =
            |key: &str| !matches!(cop_config.get(key), Some(Value::Bool(false) | Value::Null));
        let safety = if flag("Safe") && flag("SafeAutoCorrect") {
            "Safe"
        } else {
            "Unsafe"
        };
        format!(
            "{cop_name}{}",
            self.color.yellow(&format!(" [{safety} Correctable]"))
        )
    }
}

/// `offense_counts.sort_by { |k, v| [-v, k] }`.
fn ordered_offense_counts(offense_counts: &HashMap<String, usize>) -> Vec<(&str, usize)> {
    let mut counts: Vec<(&str, usize)> = offense_counts
        .iter()
        .map(|(cop, count)| (cop.as_str(), *count))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    counts
}

impl Formatter for OffenseCountFormatter {
    fn started(&mut self, _target_files: &[PathBuf]) {
        self.offense_counts.clear();
        self.offending_files_count = 0;
    }

    fn file_finished(&mut self, _file: &Path, offenses: &[Offense]) {
        for offense in offenses {
            *self
                .offense_counts
                .entry(offense.cop_name.clone())
                .or_default() += 1;
        }
        if !offenses.is_empty() {
            self.offending_files_count += 1;
        }
    }

    fn set_default_configuration(&mut self, default_configuration: &Mapping) {
        self.default_configuration = Some(default_configuration.clone());
    }

    fn finished(&mut self, _inspected_files: &[PathBuf]) {
        let counts = std::mem::take(&mut self.offense_counts);
        self.report_summary(&counts, self.offending_files_count);
        self.offense_counts = counts;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Default)]
    struct Shared(Rc<RefCell<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn formatter() -> (OffenseCountFormatter, Shared) {
        let output = Shared::default();
        let mut formatter =
            OffenseCountFormatter::new(Box::new(output.clone()), Colorizable { enabled: false });
        let mut loader = crate::config_loader::ConfigLoader::default();
        formatter.set_default_configuration(loader.default_configuration());
        (formatter, output)
    }

    fn offense(cop_name: &str) -> Offense {
        Offense {
            severity: crate::cop::Severity::Convention,
            location: crate::cop::offense::Location::no_location(),
            message: "Unwanted.".to_owned(),
            cop_name: cop_name.to_owned(),
            status: crate::cop::offense::Status::Uncorrected,
        }
    }

    fn text(output: &Shared) -> String {
        String::from_utf8(output.0.borrow().clone()).unwrap()
    }

    // spec/rubocop/formatter/offense_count_formatter_spec.rb
    #[test]
    fn file_finished_counts_offenses() {
        let (mut formatter, _) = formatter();
        formatter.started(&[]);
        formatter.file_finished(Path::new("a.rb"), &[]);
        assert!(formatter.offense_counts().is_empty());
        formatter.file_finished(Path::new("a.rb"), &[offense("OffendedCop")]);
        assert_eq!(formatter.offense_counts()["OffendedCop"], 1);
    }

    #[test]
    fn report_summary_shows_the_cop_and_the_offense_count() {
        let (mut formatter, output) = formatter();
        formatter.started(&[]);
        let counts = HashMap::from([("Style/FrozenStringLiteralComment".to_owned(), 3)]);
        formatter.report_summary(&counts, 2);
        assert_eq!(
            text(&output),
            "\n3  Style/FrozenStringLiteralComment [Unsafe Correctable]\n--\n3  Total in 2 files\n\n"
        );
    }

    #[test]
    fn finished_sorts_by_offense_count_then_cop_name() {
        let (mut formatter, output) = formatter();
        formatter.started(&[]);
        let offenses: Vec<Offense> = [
            "Style/AndOr",
            "Style/HashSyntax",
            "Naming/ConstantName",
            "Naming/ConstantName",
        ]
        .into_iter()
        .map(offense)
        .collect();
        formatter.file_finished(Path::new("lib/rubocop.rb"), &offenses);
        formatter.finished(&[]);
        assert_eq!(
            text(&output),
            "\n2  Naming/ConstantName\n1  Style/AndOr [Unsafe Correctable]\n\
             1  Style/HashSyntax [Safe Correctable]\n--\n4  Total in 1 files\n\n"
        );
    }

    #[test]
    fn column_width_follows_the_total() {
        let (mut formatter, output) = formatter();
        let counts = HashMap::from([
            ("Layout/LeadingCommentSpace".to_owned(), 1),
            ("Naming/ConstantName".to_owned(), 114),
        ]);
        formatter.report_summary(&counts, 65);
        assert_eq!(
            text(&output),
            "\n114  Naming/ConstantName\n1    Layout/LeadingCommentSpace [Safe Correctable]\n\
             --\n115  Total in 65 files\n\n"
        );
    }
}
