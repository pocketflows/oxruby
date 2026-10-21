//! Port of `lib/rubocop/cop/utils/format_string.rb`: parses `format`/`%`
//! format strings into their format sequences.

use std::collections::HashMap;
use std::sync::LazyLock;

const DIGIT_DOLLAR: &str = r"(?-mix:(?<arg_number>\d+)\$)";
const INTERPOLATION: &str = r"(?-mix:\#\{.*?\})";
const TYPE: &str = r"(?-mix:(?<type>[bBdiouxXeEfgGaAcps]))";
const NAME: &str = r"(?-mix:<(?<name>\w+)>)";
const TEMPLATE_NAME: &str = r"(?-mix:(?<!\#)\{(?<name>\w+)\})";

/// `SEQUENCE`, with the interpolated constants spelled out the way Ruby
/// embeds a `Regexp` in another (`(?-mix:...)`).
static SEQUENCE: LazyLock<Sequence> = LazyLock::new(|| {
    let flag = format!(r"(?-mix:[ #0+-]|{DIGIT_DOLLAR})");
    let number_arg = format!(r"(?-mix:\*{DIGIT_DOLLAR}?)");
    let number = format!(r"(?-mix:\d+|{number_arg}|{INTERPOLATION})");
    let width = format!(r"(?-mix:(?<width>{number}))");
    let precision = format!(r"(?-mix:\.(?<precision>{number}?))");
    let pattern = format!(
        r"
            % (?<type>%)
          | % (?<flags>{flag}*)
            (?:
              (?: {width}? {precision}? {NAME}?
                | {width}? {NAME} {precision}?
                | {NAME} (?<more_flags>{flag}*) {width}? {precision}?
              ) {TYPE}
              | {width}? {precision}? {TEMPLATE_NAME}
            )
        "
    );
    let regex = onig::Regex::with_options(
        &pattern,
        onig::RegexOptions::REGEX_OPTION_EXTEND,
        onig::Syntax::ruby(),
    )
    .expect("FormatString::SEQUENCE");
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    regex.foreach_name(|name, indices| {
        groups.insert(
            name.to_owned(),
            indices.iter().map(|&i| i as usize).collect(),
        );
        true
    });
    Sequence { regex, groups }
});

struct Sequence {
    regex: onig::Regex,
    /// The group numbers of each name (names are defined several times).
    groups: HashMap<String, Vec<usize>>,
}

impl Sequence {
    /// `match[:name]`: the last group of that name that matched.
    fn named<'t>(&self, captures: &onig::Captures<'t>, name: &str) -> Option<&'t str> {
        self.groups
            .get(name)?
            .iter()
            .rev()
            .find_map(|&i| captures.at(i))
    }
}

/// `FormatString::FormatSequence`.
#[derive(Debug, Clone)]
pub struct FormatSequence {
    source: String,
    /// Byte offsets in the format string.
    pub begin_pos: usize,
    pub end_pos: usize,
    pub flags: String,
    pub width: Option<String>,
    pub precision: Option<String>,
    pub name: Option<String>,
    pub ty: Option<String>,
    pub arg_number: Option<String>,
}

impl FormatSequence {
    pub fn is_percent(&self) -> bool {
        self.ty.as_deref() == Some("%")
    }

    pub fn is_annotated(&self) -> bool {
        self.name.is_some() && self.source.contains('<')
    }

    pub fn is_template(&self) -> bool {
        self.name.is_some() && self.source.contains('{')
    }

    pub fn is_variable_width(&self) -> bool {
        self.width.as_deref().is_some_and(|w| w.starts_with('*'))
    }

    pub fn variable_width_argument_number(&self) -> Option<i64> {
        if !self.is_variable_width() {
            return None;
        }
        let width = self.width.as_deref()?;
        if width == "*" {
            return Some(1);
        }
        let digits: String = width
            .trim_start_matches('*')
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    }

    /// `arity`: one plus the number of `*`s.
    pub fn arity(&self) -> usize {
        self.source.matches('*').count() + 1
    }

    /// `max_digit_dollar_num`.
    pub fn max_digit_dollar_num(&self) -> Option<u64> {
        crate::regex!(r"(\d+)\$")
            .captures_iter(&self.source)
            .filter_map(|c| c.at(1).and_then(|d| d.parse::<u64>().ok()))
            .max()
    }

    /// `style`: `annotated`, `template` or `unannotated`.
    pub fn style(&self) -> &'static str {
        if self.is_annotated() {
            "annotated"
        } else if self.is_template() {
            "template"
        } else {
            "unannotated"
        }
    }
}

/// `RuboCop::Cop::Utils::FormatString`.
pub struct FormatString {
    format_sequences: Vec<FormatSequence>,
}

impl FormatString {
    pub fn new(string: &str) -> Self {
        Self {
            format_sequences: parse(string),
        }
    }

    pub fn format_sequences(&self) -> &[FormatSequence] {
        &self.format_sequences
    }

    pub fn is_valid(&self) -> bool {
        !self.mixed_formats()
    }

    pub fn is_named_interpolation(&self) -> bool {
        self.format_sequences.iter().any(|seq| seq.name.is_some())
    }

    pub fn max_digit_dollar_num(&self) -> Option<u64> {
        self.format_sequences
            .iter()
            .filter_map(FormatSequence::max_digit_dollar_num)
            .max()
    }

    fn mixed_formats(&self) -> bool {
        let mut formats: Vec<&str> = self
            .format_sequences
            .iter()
            .filter(|seq| !seq.is_percent())
            .map(|seq| {
                if seq.name.is_some() {
                    "named"
                } else if seq.max_digit_dollar_num().is_some() {
                    "numbered"
                } else {
                    "unnumbered"
                }
            })
            .collect();
        formats.sort_unstable();
        formats.dedup();
        formats.len() > 1
    }
}

fn parse(source: &str) -> Vec<FormatSequence> {
    let sequence = &*SEQUENCE;
    sequence
        .regex
        .captures_iter(source)
        .map(|captures| {
            let (begin_pos, end_pos) = captures.pos(0).unwrap();
            let named = |name: &str| sequence.named(&captures, name).map(str::to_owned);
            FormatSequence {
                source: captures.at(0).unwrap_or_default().to_owned(),
                begin_pos,
                end_pos,
                flags: format!(
                    "{}{}",
                    named("flags").unwrap_or_default(),
                    named("more_flags").unwrap_or_default()
                ),
                width: named("width"),
                precision: named("precision"),
                name: named("name"),
                ty: named("type"),
                arg_number: named("arg_number"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sequences() {
        let fs = FormatString::new("%s and %<name>d and %{x} %%");
        let seqs = fs.format_sequences();
        assert_eq!(seqs.len(), 4);
        assert_eq!(seqs[0].ty.as_deref(), Some("s"));
        assert_eq!(seqs[1].name.as_deref(), Some("name"));
        assert!(seqs[1].is_annotated());
        assert!(seqs[2].is_template());
        assert!(seqs[3].is_percent());
        assert!(!fs.is_valid());
        let fs = FormatString::new("%1$s %2$*3$d");
        assert_eq!(fs.max_digit_dollar_num(), Some(3));
        assert_eq!(fs.format_sequences()[1].arity(), 2);
    }
}
