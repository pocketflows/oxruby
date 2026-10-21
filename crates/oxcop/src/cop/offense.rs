//! Port of `lib/rubocop/cop/offense.rb`.

use std::cmp::Ordering;

use crate::cop::Severity;
use crate::processed_source::{ProcessedSource, Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unsupported,
    Uncorrected,
    Corrected,
}

/// Everything formatters need about an offense's location, captured eagerly
/// so offenses outlive the source they were found in.
#[derive(Debug, Clone)]
pub struct Location {
    pub range: Range,
    pub line: usize,
    pub column: usize,
    pub last_line: usize,
    pub last_column: usize,
    /// Length in characters.
    pub length: usize,
    pub source_line: String,
    /// The offending source text (characters within the range).
    pub source: String,
}

impl Location {
    pub fn new(processed_source: &ProcessedSource<'_>, range: Range) -> Self {
        let line = processed_source.line_of(range.begin_pos);
        let last_line = processed_source.line_of(range.end_pos);
        let source = processed_source.slice(range).to_owned();
        Self {
            range,
            line,
            column: processed_source.column_of(range.begin_pos),
            last_line,
            last_column: processed_source.column_of(range.end_pos),
            length: source.chars().count(),
            source_line: processed_source.source_line(line).to_owned(),
            source,
        }
    }

    pub fn single_line(&self) -> bool {
        self.line == self.last_line
    }
}

#[derive(Debug, Clone)]
pub struct Offense {
    pub severity: Severity,
    pub location: Location,
    pub message: String,
    pub cop_name: String,
    pub status: Status,
}

impl Offense {
    pub fn line(&self) -> usize {
        self.location.line
    }

    pub fn column(&self) -> usize {
        self.location.column
    }

    pub fn real_column(&self) -> usize {
        self.column() + 1
    }

    pub fn last_line(&self) -> usize {
        self.location.last_line
    }

    pub fn last_column(&self) -> usize {
        self.location.last_column
    }

    pub fn real_last_column(&self) -> usize {
        if self.last_column() == 0 {
            1
        } else {
            self.last_column()
        }
    }

    pub fn column_length(&self) -> usize {
        if self.location.single_line() {
            self.last_column() - self.column()
        } else {
            self.location
                .source_line
                .chars()
                .count()
                .saturating_sub(self.column())
        }
    }

    /// The part of the first source line that is highlighted.
    pub fn highlighted_area(&self) -> (String, String) {
        let chars: Vec<char> = self.location.source_line.chars().collect();
        let begin = self.column().min(chars.len());
        let end = (self.column() + self.column_length()).min(chars.len());
        (
            chars[..begin].iter().collect(),
            chars[begin..end].iter().collect(),
        )
    }

    pub fn correctable(&self) -> bool {
        self.status != Status::Unsupported
    }

    pub fn corrected(&self) -> bool {
        self.status == Status::Corrected
    }

    fn comparison_key(&self) -> (usize, usize, &str, &str, Severity) {
        (
            self.line(),
            self.column(),
            &self.cop_name,
            &self.message,
            self.severity,
        )
    }
}

impl PartialEq for Offense {
    fn eq(&self, other: &Self) -> bool {
        self.comparison_key() == other.comparison_key()
    }
}

impl Eq for Offense {}

impl PartialOrd for Offense {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Offense {
    fn cmp(&self, other: &Self) -> Ordering {
        self.comparison_key().cmp(&other.comparison_key())
    }
}
