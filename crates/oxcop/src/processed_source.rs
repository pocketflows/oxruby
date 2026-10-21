//! Port of `RuboCop::AST::ProcessedSource` (from rubocop-ast), backed by
//! Prism instead of the `parser` gem.

use std::path::{Path, PathBuf};

use ruby_prism::ParseResult;

/// A half-open byte range into the source, like `Parser::Source::Range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Range {
    pub begin_pos: usize,
    pub end_pos: usize,
}

impl Range {
    pub fn new(begin_pos: usize, end_pos: usize) -> Self {
        Self { begin_pos, end_pos }
    }

    pub fn len(&self) -> usize {
        self.end_pos - self.begin_pos
    }

    pub fn is_empty(&self) -> bool {
        self.begin_pos == self.end_pos
    }
}

impl From<&ruby_prism::Location<'_>> for Range {
    fn from(location: &ruby_prism::Location<'_>) -> Self {
        Self::new(location.start_offset(), location.end_offset())
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: Range,
}

pub struct ProcessedSource<'src> {
    path: PathBuf,
    raw_source: &'src str,
    parse_result: ParseResult<'src>,
    line_starts: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
    lines: Vec<&'src str>,
    ruby_version: f64,
}

impl<'src> ProcessedSource<'src> {
    pub fn new(raw_source: &'src str, ruby_version: f64, path: &Path) -> Self {
        let parse_result = ruby_prism::parse(raw_source.as_bytes());
        let diagnostics = parse_result
            .errors()
            .map(|d| Diagnostic {
                message: d.message().to_owned(),
                range: Range::from(&d.location()),
            })
            .collect();

        let mut line_starts = vec![0];
        line_starts.extend(raw_source.match_indices('\n').map(|(i, _)| i + 1));

        // `ProcessedSource#lines` stops at `__END__`.
        let end_line = parse_result
            .data_loc()
            .map(|loc| line_index(&line_starts, loc.start_offset()));
        let mut lines: Vec<&str> = raw_source
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .collect();
        if let Some(end_line) = end_line {
            lines.truncate(end_line);
        }

        Self {
            path: path.to_path_buf(),
            raw_source,
            parse_result,
            line_starts,
            diagnostics,
            lines,
            ruby_version,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn raw_source(&self) -> &'src str {
        self.raw_source
    }

    pub fn ruby_version(&self) -> f64 {
        self.ruby_version
    }

    pub fn ast(&self) -> ruby_prism::Node<'_> {
        self.parse_result.node()
    }

    pub fn parse_result(&self) -> &ParseResult<'src> {
        &self.parse_result
    }

    pub fn valid_syntax(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Source lines without their terminators, as in RuboCop.
    pub fn lines(&self) -> &[&'src str] {
        &self.lines
    }

    /// The range of the given 1-based line, excluding its newline.
    pub fn line_range(&self, line: usize) -> Range {
        let begin = self.line_starts[line - 1];
        Range::new(
            begin,
            begin + self.lines.get(line - 1).map_or(0, |l| l.len()),
        )
    }

    /// 1-based line number for a byte offset.
    pub fn line_of(&self, pos: usize) -> usize {
        line_index(&self.line_starts, pos) + 1
    }

    /// 0-based column in characters for a byte offset.
    pub fn column_of(&self, pos: usize) -> usize {
        let start = self.line_starts[line_index(&self.line_starts, pos)];
        self.raw_source[start..pos].chars().count()
    }

    /// Full text of the 1-based line (without terminator).
    pub fn source_line(&self, line: usize) -> &'src str {
        let start = self.line_starts[line - 1];
        let rest = &self.raw_source[start..];
        let line = rest.split('\n').next().unwrap_or("");
        line.strip_suffix('\r').unwrap_or(line)
    }

    pub fn slice(&self, range: Range) -> &'src str {
        &self.raw_source[range.begin_pos..range.end_pos]
    }
}

fn line_index(line_starts: &[usize], pos: usize) -> usize {
    match line_starts.binary_search(&pos) {
        Ok(i) => i,
        Err(i) => i - 1,
    }
}
