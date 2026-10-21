//! Port of `lib/rubocop/cop/layout/extra_spacing.rb`.

use std::collections::{HashMap, HashSet};

use crate::ast::NodeType;
use crate::ast::token::Token;
use crate::cop::mixin::preceding_following_alignment::{
    ASSIGNMENT_OR_COMPARISON_TOKENS, Aligned, PrecedingFollowingAlignment, allow_for_alignment,
    assignment_tokens,
};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG_UNNECESSARY: &str = "Unnecessary spacing detected.";

/// `typed[[column, type]]`: the lines and spacings of tokens of a type at a
/// column.
type Typed = HashMap<(usize, &'static str), (Vec<usize>, Vec<isize>)>;

/// `by_column` and `typed` of a group profile.
#[derive(Default)]
struct GroupProfile {
    by_column: HashMap<usize, Vec<(usize, isize)>>,
    typed: Typed,
}

#[derive(Default)]
pub struct ExtraSpacing {
    alignment: PrecedingFollowingAlignment,
    /// `@ignored_ranges`, memoized for the cop's lifetime as in Ruby.
    ignored_ranges: Option<Vec<std::ops::Range<usize>>>,
}

/// Per-investigation state (`@aligned_comments`, `@corrected`, ...).
#[derive(Default)]
struct State {
    aligned_comments: HashSet<usize>,
    group_profiles: HashMap<Vec<usize>, GroupProfile>,
    alignment_verdicts: HashMap<(Vec<usize>, usize, &'static str), bool>,
    tokens_by_line: HashMap<usize, Vec<usize>>,
}

impl Cop for ExtraSpacing {
    fn name(&self) -> &'static str {
        "Layout/ExtraSpacing"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.is_blank() {
            return;
        }
        let mut state = State {
            aligned_comments: aligned_locations(ps),
            ..State::default()
        };
        for (index, token) in ps.tokens().iter().enumerate() {
            state
                .tokens_by_line
                .entry(token.line(ps))
                .or_default()
                .push(index);
        }
        // `@corrected`: tokens already moved by an earlier correction.
        let corrected = std::cell::RefCell::new(HashSet::new());
        let tokens = ps.tokens();
        for pair in tokens.windows(2) {
            self.check_tokens(cx, &mut state, &corrected, &pair[0], &pair[1]);
        }
    }
}

fn aligned_locations(ps: &ProcessedSource<'_>) -> HashSet<usize> {
    let mut aligned = HashSet::new();
    for pair in ps.comments().windows(2) {
        let (a, b) = (pair[0].range, pair[1].range);
        if ps.column_of(a.begin_pos) == ps.column_of(b.begin_pos) {
            aligned.insert(ps.line_of(a.begin_pos));
            aligned.insert(ps.line_of(b.begin_pos));
        }
    }
    aligned
}

/// `token.begin_pos - previous_token.end_pos`, in characters.
fn spacing_between(ps: &ProcessedSource<'_>, previous_end: usize, begin: usize) -> isize {
    let source = ps.buffer_source();
    if previous_end <= begin {
        source[previous_end..begin].chars().count() as isize
    } else {
        -(source[begin..previous_end].chars().count() as isize)
    }
}

impl ExtraSpacing {
    fn check_tokens(
        &mut self,
        cx: &mut Investigation<'_, '_>,
        state: &mut State,
        corrected: &std::cell::RefCell<HashSet<Range>>,
        token1: &Token,
        token2: &Token,
    ) {
        if token2.ty == "tNL" {
            return;
        }
        let ps = cx.processed_source;
        if force_equal_sign_alignment(cx)
            && assignment_tokens(ps)
                .iter()
                .any(|t| t.range == token2.range && t.ty == token2.ty)
        {
            self.check_assignment(cx, corrected, token2);
        } else {
            self.check_other(cx, state, token1, token2);
        }
    }

    fn check_assignment(
        &mut self,
        cx: &mut Investigation<'_, '_>,
        corrected: &std::cell::RefCell<HashSet<Range>>,
        token: &Token,
    ) {
        let ps = cx.processed_source;
        if self.alignment.aligned_with_preceding_equals_operator(ps, token.range) != Aligned::No {
            return;
        }
        let message = "`=` is not aligned with the preceding assignment.";
        let line = token.line(ps);
        let lines = self.all_relevant_assignment_lines(ps, line);
        cx.add_offense_with_correction(token.range, message, |corrector| {
            align_equal_signs(ps, corrector, corrected, &lines);
        });
    }

    fn check_other(&mut self, cx: &mut Investigation<'_, '_>, state: &mut State, token1: &Token, token2: &Token) {
        if cx.cop_config_bool("AllowBeforeTrailingComments", false) && token2.text.starts_with('#') {
            return;
        }
        let Some(range) = self.extra_space_range(cx, state, token1, token2) else {
            return;
        };
        if self.ignored_range(cx.processed_source, range.begin_pos) {
            return;
        }
        cx.add_offense_with_correction(range, MSG_UNNECESSARY, |corrector| corrector.remove(range));
    }

    fn extra_space_range(
        &mut self,
        cx: &Investigation<'_, '_>,
        state: &mut State,
        token1: &Token,
        token2: &Token,
    ) -> Option<Range> {
        let ps = cx.processed_source;
        if token1.line(ps) != token2.line(ps) {
            return None;
        }
        let start_pos = token1.end_pos();
        let end_pos = token2.begin_pos().checked_sub(1)?;
        if end_pos <= start_pos {
            return None;
        }
        if allow_for_alignment(cx) && self.aligned_tok(ps, state, token1, token2) {
            return None;
        }
        Some(range_between(start_pos, end_pos))
    }

    fn aligned_tok(&mut self, ps: &ProcessedSource<'_>, state: &mut State, previous_token: &Token, token: &Token) -> bool {
        if token.is_comment() {
            return state.aligned_comments.contains(&token.line(ps));
        }
        self.alignment.aligned_with_something(ps, token.range)
            && (ASSIGNMENT_OR_COMPARISON_TOKENS.contains(&token.ty)
                || self.spacing_varies(ps, state, previous_token, token))
    }

    fn spacing_varies(&mut self, ps: &ProcessedSource<'_>, state: &mut State, previous_token: &Token, token: &Token) -> bool {
        let group = self.group_profile(ps, state, token.line(ps));
        let column = token.column(ps);
        let profile = &state.group_profiles[&group];
        let (lines, spacings) = profile
            .typed
            .get(&(column, token.ty))
            .cloned()
            .unwrap_or_default();
        let entries = profile.by_column.get(&column).cloned().unwrap_or_default();
        if lines.len() > 1 {
            let key = (group.clone(), column, token.ty);
            if let Some(&verdict) = state.alignment_verdicts.get(&key) {
                return verdict;
            }
            let verdict = distinct_count(&spacings) > 1
                || aligned_on_other_column(&state.group_profiles[&group], column, &lines);
            state.alignment_verdicts.insert(key, verdict);
            verdict
        } else if entries.len() > 1 {
            let spacings: Vec<isize> = entries.iter().map(|(_, s)| *s).collect();
            distinct_count(&spacings) > 1
        } else {
            self.nearest_spacing_varies(ps, state, previous_token, token)
        }
    }

    fn group_profile(&mut self, ps: &ProcessedSource<'_>, state: &mut State, line_number: usize) -> Vec<usize> {
        let group = self.alignment.alignment_lines(ps, line_number);
        if !state.group_profiles.contains_key(&group) {
            let profile = build_group_profile(ps, state, &group);
            state.group_profiles.insert(group.clone(), profile);
        }
        group
    }

    fn nearest_spacing_varies(&mut self, ps: &ProcessedSource<'_>, state: &State, previous_token: &Token, token: &Token) -> bool {
        let spacing = spacing_between(ps, previous_token.end_pos(), token.begin_pos());
        let line = token.line(ps);
        let down: Vec<usize> = (1..line).rev().collect();
        let up: Vec<usize> = ((line + 1)..=ps.lines().len()).collect();
        [down, up]
            .iter()
            .any(|line_numbers| self.nearest_spacing_differs(ps, state, line_numbers, token, spacing))
    }

    fn nearest_spacing_differs(
        &mut self,
        ps: &ProcessedSource<'_>,
        state: &State,
        line_numbers: &[usize],
        token: &Token,
        spacing: isize,
    ) -> bool {
        let column = token.column(ps);
        for &line in line_numbers {
            if self.alignment.definition_boundary_lines(ps).contains(&line) {
                break;
            }
            if self.alignment.aligned_comment_lines(ps).contains(&line) {
                continue;
            }
            if let Some(other_spacing) = spacing_before_token_at(ps, state, line, column) {
                return other_spacing != spacing;
            }
        }
        false
    }

    fn ignored_range(&mut self, ps: &ProcessedSource<'_>, start_pos: usize) -> bool {
        let Some(ast) = ps.ast() else {
            return false;
        };
        let ranges = self.ignored_ranges.get_or_insert_with(|| {
            ast.each_node()
                .filter(|n| n.is(NodeType::Pair))
                .filter(|pair| !pair.parent().is_some_and(|p| p.is_single_line()))
                .filter_map(|pair| {
                    let key = pair.key()?;
                    let value = pair.pair_value()?;
                    Some(key.source_range().end_pos..value.source_range().begin_pos)
                })
                .collect()
        });
        ranges.iter().any(|r| r.contains(&start_pos))
    }

    fn all_relevant_assignment_lines(&mut self, ps: &ProcessedSource<'_>, line_number: usize) -> Vec<usize> {
        let last_line_number = ps.lines().len();
        let down: Vec<usize> = (1..=line_number).rev().collect();
        let up: Vec<usize> = (line_number..=last_line_number).collect();
        let mut lines = self.alignment.relevant_assignment_lines(ps, &down);
        lines.extend(self.alignment.relevant_assignment_lines(ps, &up));
        lines.sort_unstable();
        lines.dedup();
        lines
    }
}

fn distinct_count(values: &[isize]) -> usize {
    let mut seen = HashSet::new();
    values.iter().filter(|v| seen.insert(**v)).count()
}

fn aligned_on_other_column(profile: &GroupProfile, column: usize, lines: &[usize]) -> bool {
    let line_set: HashSet<usize> = lines.iter().copied().collect();
    profile.by_column.iter().any(|(&other, entries)| {
        if other == column {
            return false;
        }
        let shared: Vec<isize> = entries
            .iter()
            .filter(|(line, _)| line_set.contains(line))
            .map(|(_, spacing)| *spacing)
            .collect();
        distinct_count(&shared) > 1
    })
}

fn build_group_profile(ps: &ProcessedSource<'_>, state: &State, group: &[usize]) -> GroupProfile {
    let mut profile = GroupProfile::default();
    let tokens = ps.tokens();
    for &line in group {
        let Some(indexes) = state.tokens_by_line.get(&line) else {
            continue;
        };
        for pair in indexes.windows(2) {
            let (previous_token, token) = (&tokens[pair[0]], &tokens[pair[1]]);
            let spacing = spacing_between(ps, previous_token.end_pos(), token.begin_pos());
            let column = token.column(ps);
            profile.by_column.entry(column).or_default().push((line, spacing));
            let entry = profile.typed.entry((column, token.ty)).or_default();
            entry.0.push(line);
            entry.1.push(spacing);
        }
    }
    profile
}

fn spacing_before_token_at(ps: &ProcessedSource<'_>, state: &State, line_number: usize, column: usize) -> Option<isize> {
    let indexes = state.tokens_by_line.get(&line_number)?;
    let tokens = ps.tokens();
    let index = indexes.iter().position(|&i| tokens[i].column(ps) == column)?;
    if index == 0 {
        return None;
    }
    let (previous, token) = (&tokens[indexes[index - 1]], &tokens[indexes[index]]);
    Some(spacing_between(ps, previous.end_pos(), token.begin_pos()))
}

fn force_equal_sign_alignment(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("ForceEqualSignAlignment", false)
}

fn align_equal_signs(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    corrected: &std::cell::RefCell<HashSet<Range>>,
    lines: &[usize],
) {
    let tokens: Vec<&Token> = assignment_tokens(ps)
        .into_iter()
        .filter(|t| lines.contains(&t.line(ps)))
        .collect();
    let Some(align_to) = tokens.iter().map(|t| align_column(ps, t)).max() else {
        return;
    };
    for token in tokens {
        if !corrected.borrow_mut().insert(token.range) {
            continue;
        }
        let diff = align_to as isize - ps.column_of(token.end_pos()) as isize;
        if diff > 0 {
            corrector.insert_before(token.range, " ".repeat(diff as usize));
        } else if diff < 0 {
            corrector.remove_preceding(token.range, diff.unsigned_abs());
        }
    }
}

fn align_column(ps: &ProcessedSource<'_>, asgn_token: &Token) -> usize {
    let line = ps.lines()[asgn_token.line(ps) - 1];
    let leading: String = line.chars().take(asgn_token.column(ps)).collect();
    let spaces = leading.len() - leading.trim_end_matches(' ').len();
    ps.column_of(asgn_token.end_pos()) - spaces + 1
}
