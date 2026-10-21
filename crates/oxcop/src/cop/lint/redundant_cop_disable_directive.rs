//! Port of `lib/rubocop/cop/lint/redundant_cop_disable_directive.rb`.
//!
//! The runner runs it after the other cops, with their offenses (disabled
//! ones included) as `offenses_to_check` (`Runner#add_redundant_disables`);
//! without them it does nothing.

use std::collections::BTreeSet;

use serde_yaml_ng::Value;

use crate::comment_config::{DirectiveRange, LINE_MAX, LINE_MIN};
use crate::cop::base::OffenseToCheck;
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_between, range_with_surrounding_comma,
    range_with_surrounding_space,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::{Comment, Range};

const COP_NAME: &str = "Lint/RedundantCopDisableDirective";
const DEPARTMENT_MARKER: &str = "DEPARTMENT";
const TRAILING_COMMENT_MARKER: &str = "--";

#[derive(Default)]
pub struct RedundantCopDisableDirective {
    offenses_to_check: Option<Vec<OffenseToCheck>>,
}

impl Cop for RedundantCopDisableDirective {
    fn name(&self) -> &'static str {
        COP_NAME
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn set_offenses_to_check(&mut self, offenses: Vec<OffenseToCheck>) {
        self.offenses_to_check = Some(offenses);
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(offenses) = &self.offenses_to_check else {
            return;
        };
        let offenses_to_add = {
            let checker = Checker { cx: &*cx, offenses };
            // Insertion-ordered, like Ruby's Hash of Sets.
            let mut redundant: Vec<(Comment<'_>, BTreeSet<String>)> = Vec::new();
            checker.each_redundant_disable(&mut |comment, cop| {
                match redundant.iter_mut().find(|(c, _)| c.range == comment.range) {
                    Some((_, cops)) => {
                        cops.insert(cop);
                    }
                    None => redundant.push((comment, BTreeSet::from([cop]))),
                }
            });
            checker.offenses_for(&redundant)
        };
        for (range, message, correction) in offenses_to_add {
            match correction {
                Some(correction) => cx.add_offense_with_correction(range, &message, |corrector| {
                    correction.apply(corrector);
                }),
                None => cx.add_offense(range, &message),
            }
        }
    }
}

/// A correction computed up front (the checker borrows the investigation).
enum Correction {
    Remove(Range),
    Replace(Range, &'static str),
}

impl Correction {
    fn apply(&self, corrector: &mut Corrector) {
        match *self {
            Correction::Remove(range) => corrector.remove(range),
            Correction::Replace(range, text) => corrector.replace(range, text),
        }
    }
}

struct Checker<'c, 'a, 's> {
    cx: &'c Investigation<'a, 's>,
    offenses: &'c [OffenseToCheck],
}

/// A line number as Ruby sees it: the open ends are ±infinity.
fn line_f(line: i64) -> f64 {
    match line {
        LINE_MIN => f64::NEG_INFINITY,
        LINE_MAX => f64::INFINITY,
        n => n as f64,
    }
}

impl<'s> Checker<'_, '_, 's> {
    fn src(&self) -> &'s str {
        self.cx.processed_source.raw_source()
    }

    fn directive(&self, comment: &Comment<'s>) -> DirectiveComment<'s> {
        DirectiveComment::new(
            comment.text,
            comment.range,
            self.cx.processed_source.line_of(comment.range.begin_pos) as i64,
        )
    }

    fn comment_at(&self, range: Range) -> Option<Comment<'s>> {
        self.cx
            .processed_source
            .comments()
            .iter()
            .find(|c| c.range == range)
            .copied()
    }

    fn all_cop_names(&self) -> &[String] {
        use crate::directive_comment::CopRegistry;
        self.cx.config.known_cops().names()
    }

    fn known_cop(&self, cop: &str) -> bool {
        self.cx.config.known_cops().contains(cop)
    }

    fn disabled_ranges(&self) -> Vec<(f64, f64)> {
        match self
            .cx
            .comment_config()
            .cop_disabled_line_ranges
            .get(COP_NAME)
        {
            Some(ranges) => ranges
                .iter()
                .map(|r| (line_f(r.begin), line_f(r.end)))
                .collect(),
            None => vec![(0.0, 0.0)],
        }
    }

    fn each_redundant_disable(&self, yield_: &mut dyn FnMut(Comment<'s>, String)) {
        self.each_detached_next_directive(yield_);
        let comment_config = self.cx.comment_config();
        let mut cops: Vec<(&String, &Vec<DirectiveRange>)> =
            comment_config.cop_disabled_line_ranges.iter().collect();
        // Ruby's Hash keeps the order in which cops first got a range.
        cops.sort_by_key(|(name, ranges)| {
            (ranges.first().map_or(i64::MAX, |r| r.begin), (*name).clone())
        });
        for (cop, line_ranges) in cops {
            if self.pending_cop_not_run(cop) {
                continue;
            }
            self.each_already_disabled(cop, line_ranges, yield_);
            self.each_line_range(cop, line_ranges, yield_);
        }
    }

    fn pending_cop_not_run(&self, cop: &str) -> bool {
        if !self.known_cop(cop) {
            return false;
        }
        let enabled = self.cx.config.for_cop(cop).get("Enabled");
        enabled.and_then(Value::as_str) == Some("pending") && !self.cx.config.enabled_pending_cop(cop)
    }

    fn each_line_range(
        &self,
        cop: &str,
        line_ranges: &[DirectiveRange],
        yield_: &mut dyn FnMut(Comment<'s>, String),
    ) {
        for (index, line_range) in line_ranges.iter().enumerate() {
            if self.should_skip_line_range(cop, line_range) {
                continue;
            }
            let Some(comment) = self.directive_comment(line_range) else {
                continue;
            };
            if self.skip_directive(&comment) {
                continue;
            }
            let next_range = line_ranges.get(index + 1);
            if let Some(redundant) =
                self.find_redundant_directive(cop, &comment, line_range, next_range)
            {
                yield_(comment, redundant);
            }
        }
    }

    fn should_skip_line_range(&self, cop: &str, line_range: &DirectiveRange) -> bool {
        self.ignore_offense(line_range) || self.expected_final_disable(cop, line_range)
    }

    /// `line_range.directive.comment`; the config-disabled one has none.
    fn directive_comment(&self, line_range: &DirectiveRange) -> Option<Comment<'s>> {
        let index = line_range.directive?;
        let directive = self.cx.comment_config().directives.get(index)?;
        self.comment_at(directive.comment_range)
    }

    fn each_detached_next_directive(&self, yield_: &mut dyn FnMut(Comment<'s>, String)) {
        let comment_config = self.cx.comment_config();
        for &index in &comment_config.detached_next_directives {
            let Some(owned) = comment_config.directives.get(index) else {
                continue;
            };
            let Some(comment) = self.comment_at(owned.comment_range) else {
                continue;
            };
            let directive = self.directive(&comment);
            let names: Vec<String> = if directive.next() {
                directive
                    .signed_args()
                    .into_iter()
                    .flat_map(|(_, names)| names)
                    .collect()
            } else {
                directive.raw_cop_names()
            };
            for cop in names {
                yield_(comment, cop);
            }
        }
    }

    fn skip_directive(&self, comment: &Comment<'s>) -> bool {
        let directive = self.directive(comment);
        directive.push() || directive.pop() || directive.next()
    }

    fn find_redundant_directive(
        &self,
        cop: &str,
        comment: &Comment<'s>,
        line_range: &DirectiveRange,
        next_range: Option<&DirectiveRange>,
    ) -> Option<String> {
        if self.all_disabled(comment) {
            self.find_redundant_all(line_range, next_range)
        } else if self.department_disabled(cop, comment) {
            self.find_redundant_department(cop, line_range)
        } else {
            self.find_redundant_cop(cop, line_range)
        }
    }

    fn each_already_disabled(
        &self,
        cop: &str,
        line_ranges: &[DirectiveRange],
        yield_: &mut dyn FnMut(Comment<'s>, String),
    ) {
        for pair in line_ranges.windows(2) {
            let (previous_range, range) = (&pair[0], &pair[1]);
            if self.ignore_offense(range) || !followed_ranges(previous_range, range) {
                continue;
            }
            let Some(comment) = self.directive_comment(range) else {
                continue;
            };
            if self.skip_directive(&comment) || self.all_disabled(&comment) {
                continue;
            }
            let redundant = if self.department_disabled(cop, &comment) {
                self.find_redundant_department(cop, range)
            } else {
                Some(cop.to_owned())
            };
            if let Some(redundant) = redundant {
                yield_(comment, redundant);
            }
        }
    }

    fn find_redundant_cop(&self, cop: &str, range: &DirectiveRange) -> Option<String> {
        let cop_offenses: Vec<&OffenseToCheck> =
            self.offenses.iter().filter(|o| o.cop_name == cop).collect();
        range_with_offense(range, &cop_offenses).then(|| cop.to_owned())
    }

    fn find_redundant_all(
        &self,
        range: &DirectiveRange,
        next_range: Option<&DirectiveRange>,
    ) -> Option<String> {
        let has_no_next_range = next_range.is_none_or(|next| !followed_ranges(range, next));
        let all: Vec<&OffenseToCheck> = self.offenses.iter().collect();
        (has_no_next_range && range_with_offense(range, &all)).then(|| "all".to_owned())
    }

    fn find_redundant_department(&self, cop: &str, range: &DirectiveRange) -> Option<String> {
        let department = cop.split('/').next().unwrap_or(cop);
        let offenses: Vec<&OffenseToCheck> = self
            .offenses
            .iter()
            .filter(|o| o.cop_name.starts_with(department))
            .collect();
        range_with_offense(range, &offenses).then(|| format!("{DEPARTMENT_MARKER}{department}"))
    }

    fn all_disabled(&self, comment: &Comment<'s>) -> bool {
        self.directive(comment).disabled_all()
    }

    fn ignore_offense(&self, line_range: &DirectiveRange) -> bool {
        if line_range.begin == LINE_MIN {
            return true;
        }
        let (min, max) = (line_f(line_range.begin), line_f(line_range.end));
        self.disabled_ranges()
            .iter()
            .any(|&(begin, end)| begin <= min && min <= end && begin <= max && max <= end)
    }

    fn expected_final_disable(&self, cop: &str, line_range: &DirectiveRange) -> bool {
        self.known_cop(cop) && !self.cx.config.cop_enabled(cop) && line_range.end == LINE_MAX
    }

    fn department_disabled(&self, cop: &str, comment: &Comment<'s>) -> bool {
        let directive = self.directive(comment);
        let registry = self.cx.config.known_cops();
        let in_department = directive
            .department_names(registry)
            .iter()
            .any(|department| cop.starts_with(department.as_str()));
        let overridden = in_department && directive.raw_cop_names().iter().any(|n| n == cop);
        in_department && !overridden
    }

    /// `add_offenses`: (range, message, correction) for each offense.
    fn offenses_for(
        &self,
        redundant: &[(Comment<'s>, BTreeSet<String>)],
    ) -> Vec<(Range, String, Option<Correction>)> {
        let mut out = Vec::new();
        for (comment, cops) in redundant {
            let directive = self.directive(comment);
            if self.all_disabled(comment) || directive.directive_count() == cops.len() {
                self.add_offense_for_entire_comment(comment, cops, &mut out);
            } else {
                self.add_offense_for_some_cops(comment, cops, &mut out);
            }
        }
        out
    }

    fn add_offense_for_entire_comment(
        &self,
        comment: &Comment<'s>,
        cops: &BTreeSet<String>,
        out: &mut Vec<(Range, String, Option<Correction>)>,
    ) {
        let directive = self.directive(comment);
        let Some(location) = directive.range() else {
            return;
        };
        let cop_names = cops
            .iter()
            .map(|c| self.describe(c))
            .collect::<Vec<_>>()
            .join(", ");
        let message = message(&cop_names, directive.enable_next());
        if cops.iter().any(|c| self.unknown_cop(c)) || self.misplaced_next_directive(&directive) {
            out.push((location, message, None));
            return;
        }
        let correction = self.remove_entire_comment(comment, &directive);
        out.push((location, message, correction));
    }

    fn remove_entire_comment(
        &self,
        comment: &Comment<'s>,
        directive: &DirectiveComment<'s>,
    ) -> Option<Correction> {
        let directive_range = range_with_reason(directive, comment)?;
        let range = self.comment_range_with_surrounding_space(directive_range, comment.range);
        Some(if leave_free_comment(comment, self.src(), range) {
            Correction::Replace(range, " # ")
        } else {
            Correction::Remove(range)
        })
    }

    fn comment_range_with_surrounding_space(&self, directive: Range, line_comment: Range) -> Range {
        let ps = self.cx.processed_source;
        let line = ps.line_of(directive.begin_pos);
        if self.previous_line_blank(line)
            && self.comment_only_line(line)
            && directive.begin_pos == line_comment.begin_pos
        {
            range_with_surrounding_space(self.src(), directive, SurroundingSpace::side(Side::Right))
        } else {
            let original_begin = directive.begin_pos;
            let range = range_with_surrounding_space(
                self.src(),
                directive,
                SurroundingSpace {
                    newlines: true,
                    ..SurroundingSpace::side(Side::Left)
                },
            );
            range_with_surrounding_space(
                self.src(),
                range,
                SurroundingSpace {
                    newlines: original_begin == 0,
                    ..SurroundingSpace::side(Side::Right)
                },
            )
        }
    }

    /// `buffer.source_line(line - 1).blank?` (line 0 is the last line, as
    /// Ruby's negative index).
    fn previous_line_blank(&self, line: usize) -> bool {
        let lines = self.cx.processed_source.lines();
        let previous = if line >= 2 {
            lines.get(line - 2)
        } else {
            lines.last()
        };
        previous.is_none_or(|l| l.trim().is_empty())
    }

    fn comment_only_line(&self, line: usize) -> bool {
        let ps = self.cx.processed_source;
        !ps.tokens()
            .iter()
            .any(|t| t.ty != "tCOMMENT" && ps.line_of(t.range.begin_pos) == line)
    }

    fn add_offense_for_some_cops(
        &self,
        comment: &Comment<'s>,
        cops: &BTreeSet<String>,
        out: &mut Vec<(Range, String, Option<Correction>)>,
    ) {
        let mut cop_ranges: Vec<(&String, Range)> = cops
            .iter()
            .filter_map(|c| self.cop_range(comment, c).map(|r| (c, r)))
            .collect();
        cop_ranges.sort_by_key(|(_, r)| r.begin_pos);
        let ranges: Vec<Range> = cop_ranges.iter().map(|(_, r)| *r).collect();
        for (index, (cop, range)) in cop_ranges.iter().enumerate() {
            let message = message(&self.describe(cop), false);
            if self.unknown_cop(cop) {
                out.push((*range, message, None));
                continue;
            }
            let removal = self.directive_range_in_list(*range, &ranges, index);
            out.push((*range, message, Some(Correction::Remove(removal))));
        }
    }

    fn directive_range_in_list(&self, range: Range, ranges: &[Range], index: usize) -> Range {
        let src = self.src();
        let mut range = range;
        if self.ends_its_line(*ranges.last().unwrap_or(&range)) && self.trailing_range(ranges, index) {
            range = range_with_surrounding_space(src, range, SurroundingSpace::side(Side::Left));
            range = range_with_surrounding_comma(src, range, Side::Left);
        }
        let range = range_with_surrounding_comma(src, range, Side::Right);
        range_with_surrounding_space(
            src,
            range,
            SurroundingSpace {
                newlines: false,
                ..SurroundingSpace::side(Side::Right)
            },
        )
    }

    fn misplaced_next_directive(&self, directive: &DirectiveComment<'_>) -> bool {
        (directive.disable_next() || directive.next() || directive.enable_next())
            && usize::try_from(directive.line_number).is_ok_and(|l| !self.comment_only_line(l))
    }

    fn unknown_cop(&self, cop: &str) -> bool {
        if cop == "all" || cop.starts_with(DEPARTMENT_MARKER) {
            return false;
        }
        !self.known_cop(cop)
    }

    fn cop_range(&self, comment: &Comment<'s>, cop: &str) -> Option<Range> {
        let cop = cop.replace(DEPARTMENT_MARKER, "");
        matching_range(comment, &cop)
            .or_else(|| matching_range(comment, cop.rsplit('/').next().unwrap_or(&cop)))
    }

    /// `ends_its_line?(range)`.
    fn ends_its_line(&self, range: Range) -> bool {
        let ps = self.cx.processed_source;
        let last_line = ps.line_of(range.end_pos.saturating_sub(1).max(range.begin_pos));
        let line = ps.source_line(last_line);
        let trimmed = line.trim_end().chars().count();
        let line_start = ps.line_start(last_line);
        let last_column = self.src()[line_start..range.end_pos].chars().count();
        trimmed == last_column
    }

    /// `trailing_range?(ranges, range)`.
    fn trailing_range(&self, ranges: &[Range], index: usize) -> bool {
        let src = self.src();
        ranges[index..].windows(2).all(|pair| {
            let between = src.get(pair[0].end_pos..pair[1].begin_pos).unwrap_or_default();
            let trimmed = between.trim();
            trimmed == ","
        })
    }

    fn describe(&self, cop: &str) -> String {
        if cop == "all" {
            return "all cops".to_owned();
        }
        if let Some(department) = cop.strip_prefix(DEPARTMENT_MARKER) {
            return format!("`{department}` department");
        }
        if self.known_cop(cop) {
            return format!("`{cop}`");
        }
        let names = self.all_cop_names();
        let basename = format!("/{}", cop.rsplit('/').next().unwrap_or(cop));
        let similar = names
            .iter()
            .find(|name| name.ends_with(&basename))
            .cloned()
            .or_else(|| {
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                crate::name_similarity::find_similar_name(cop, &names)
            });
        match similar {
            Some(similar) => format!("`{cop}` (did you mean `{similar}`?)"),
            None => format!("`{cop}` (unknown cop)"),
        }
    }
}

fn followed_ranges(range: &DirectiveRange, next_range: &DirectiveRange) -> bool {
    range.end == next_range.begin
}

/// `range_with_offense?`: no offense overlaps the range.
fn range_with_offense(range: &DirectiveRange, offenses: &[&OffenseToCheck]) -> bool {
    let (begin, end) = (line_f(range.begin), line_f(range.end));
    !offenses
        .iter()
        .any(|o| (o.line as f64) <= end && (o.last_line as f64) >= begin)
}

fn message(cop_names: &str, enabling: bool) -> String {
    format!(
        "Unnecessary {} of {cop_names}.",
        if enabling { "enabling" } else { "disabling" }
    )
}

/// `DirectiveComment#range_with_reason`.
fn range_with_reason(directive: &DirectiveComment<'_>, comment: &Comment<'_>) -> Option<Range> {
    let range = directive.range()?;
    let trailing = comment
        .text
        .get(range.end_pos - comment.range.begin_pos..)
        .unwrap_or_default();
    if trailing.trim_start().starts_with(TRAILING_COMMENT_MARKER) {
        Some(range_between(range.begin_pos, comment.range.end_pos))
    } else {
        Some(range)
    }
}

/// `leave_free_comment?(comment, range)`.
fn leave_free_comment(comment: &Comment<'_>, src: &str, range: Range) -> bool {
    let removed = src.get(range.begin_pos..range.end_pos).unwrap_or_default().trim();
    let free_comment = if removed.is_empty() {
        comment.text.to_owned()
    } else {
        comment.text.replace(removed, "")
    };
    !free_comment.is_empty() && !free_comment.starts_with('#')
}

/// `matching_range(comment.source_range, needle)`: `needle` not followed by
/// a word character.
fn matching_range(comment: &Comment<'_>, needle: &str) -> Option<Range> {
    let text = comment.text;
    let mut from = 0;
    while let Some(i) = text.get(from..)?.find(needle) {
        let at = from + i;
        let next = text[at + needle.len()..].chars().next();
        if !next.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            let begin = comment.range.begin_pos + at;
            return Some(range_between(begin, begin + needle.len()));
        }
        from = at + 1;
    }
    None
}
