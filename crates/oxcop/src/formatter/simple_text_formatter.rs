//! Port of `lib/rubocop/formatter/simple_text_formatter.rb` (and
//! `quiet_formatter.rb`, which only differs in hiding a clean summary).

use std::io::Write;
use std::path::{Path, PathBuf};

use super::text_util::pluralize;
use super::{Colorizable, Formatter, FormatterOptions};
use crate::cop::Offense;
use crate::path_util::smart_path;

/// Offense counters shared by the text formatters.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub offenses: usize,
    pub corrections: usize,
    pub correctable: usize,
}

impl Stats {
    pub fn count(&mut self, offenses: &[Offense]) {
        self.offenses += offenses.len();
        let corrected = offenses.iter().filter(|o| o.corrected()).count();
        self.corrections += corrected;
        self.correctable += offenses.iter().filter(|o| o.correctable()).count() - corrected;
    }
}

pub fn message(color: &Colorizable, offense: &Offense) -> String {
    let prefix = if offense.corrected() {
        color.green("[Corrected] ")
    } else if offense.correctable() {
        color.yellow("[Correctable] ")
    } else {
        String::new()
    };
    format!("{prefix}{}", annotate_message(color, &offense.message))
}

/// Highlights `backticked` code in messages. As in RuboCop's
/// `gsub(/`(.*?)`/m, yellow('\1'))`, the backticks go even without color.
fn annotate_message(color: &Colorizable, message: &str) -> String {
    let mut out = String::new();
    let mut rest = message;
    while let Some(start) = rest.find('`') {
        let Some(len) = rest[start + 1..].find('`') else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&color.yellow(&rest[start + 1..start + 1 + len]));
        rest = &rest[start + 2 + len..];
    }
    out.push_str(rest);
    out
}

/// Port of `SimpleTextFormatter::Report#summary`.
pub fn summary(
    color: &Colorizable,
    options: FormatterOptions,
    file_count: usize,
    stats: Stats,
) -> String {
    let files = pluralize(file_count, "file", false);
    let offenses = {
        let text = pluralize(stats.offenses, "offense", true);
        if stats.offenses == 0 {
            color.green(&text)
        } else {
            color.red(&text)
        }
    };
    let corrections = || {
        let text = pluralize(stats.corrections, "offense", false);
        if stats.corrections == stats.offenses {
            color.green(&text)
        } else {
            color.cyan(&text)
        }
    };
    let correctable = || {
        if options.safe_autocorrect {
            let text = pluralize(stats.correctable, "more offense", false);
            format!("{} can be corrected with `rubocop -A`", color.yellow(&text))
        } else {
            let text = pluralize(stats.correctable, "offense", false);
            format!("{} autocorrectable", color.yellow(&text))
        }
    };

    if stats.corrections > 0 {
        if stats.correctable > 0 {
            format!(
                "{files} inspected, {offenses} detected, {} corrected, {}",
                corrections(),
                correctable()
            )
        } else {
            format!(
                "{files} inspected, {offenses} detected, {} corrected",
                corrections()
            )
        }
    } else if stats.correctable > 0 {
        format!("{files} inspected, {offenses} detected, {}", correctable())
    } else {
        format!("{files} inspected, {offenses} detected")
    }
}

pub struct SimpleTextFormatter {
    output: Box<dyn Write>,
    color: Colorizable,
    options: FormatterOptions,
    stats: Stats,
    quiet: bool,
}

impl SimpleTextFormatter {
    pub fn new(
        output: Box<dyn Write>,
        color: Colorizable,
        options: FormatterOptions,
        quiet: bool,
    ) -> Self {
        Self {
            output,
            color,
            options,
            stats: Stats::default(),
            quiet,
        }
    }
}

impl Formatter for SimpleTextFormatter {
    fn file_finished(&mut self, file: &Path, offenses: &[Offense]) {
        if offenses.is_empty() {
            return;
        }
        self.stats.count(offenses);
        let _ = writeln!(
            self.output,
            "{}",
            self.color.yellow(&format!("== {} ==", smart_path(file)))
        );
        for offense in offenses {
            let _ = writeln!(
                self.output,
                "{}:{:>3}:{:>3}: {}",
                self.color.severity_code(offense.severity),
                offense.line(),
                offense.real_column(),
                message(&self.color, offense)
            );
        }
    }

    fn finished(&mut self, inspected_files: &[PathBuf]) {
        if self.quiet && self.stats.offenses == 0 {
            return;
        }
        let summary = summary(&self.color, self.options, inspected_files.len(), self.stats);
        let _ = write!(self.output, "\n{summary}\n");
        let _ = self.output.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotate_message_strips_backticks_like_rubocop() {
        let plain = Colorizable { enabled: false };
        assert_eq!(
            annotate_message(&plain, "Use `==` if you meant `a\nb`."),
            "Use == if you meant a\nb."
        );
        assert_eq!(annotate_message(&plain, "a `` b `c"), "a  b `c");
        let color = Colorizable { enabled: true };
        assert_eq!(
            annotate_message(&color, "Missing space after `#`."),
            "Missing space after \x1b[33m#\x1b[0m."
        );
    }
}
