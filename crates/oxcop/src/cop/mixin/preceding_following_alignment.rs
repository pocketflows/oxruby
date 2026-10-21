//! Port of `lib/rubocop/cop/mixin/preceding_following_alignment.rb`.
//!
//! The Ruby mixin memoizes several line sets in instance variables for the
//! cop's lifetime; a cop keeps a [`PrecedingFollowingAlignment`] field for
//! them. Line numbers are 1-based, columns in characters.

use std::collections::{HashMap, HashSet};

use crate::ast::NodeType;
use crate::ast::token::Token;
use crate::cop::Investigation;
use crate::cop::mixin::util::begins_its_line;
use crate::processed_source::{ProcessedSource, Range};

pub const ASSIGNMENT_OR_COMPARISON_TOKENS: &[&str] = &[
    "tEQL", "tEQ", "tEQQ", "tNEQ", "tLEQ", "tGEQ", "tOP_ASGN", "tLSHFT",
];

/// The result of `aligned_with_{preceding,subsequent}_equals_operator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aligned {
    Yes,
    No,
    None,
}

#[derive(Clone, Copy)]
enum Predicate {
    /// `aligned_token?`
    Token,
    /// `aligned_operator?`
    Operator,
}

#[derive(Debug, Default)]
pub struct PrecedingFollowingAlignment {
    aligned_comment_lines: Option<HashSet<usize>>,
    assignment_lines: Option<HashSet<usize>>,
    interrupting_operator_lines: Option<HashSet<usize>>,
    alignment_lines_by_line: HashMap<usize, Vec<usize>>,
    definition_boundary_lines: Option<HashSet<usize>>,
}

/// `allow_for_alignment?`
pub fn allow_for_alignment(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowForAlignment", false)
}

fn first_non_space(line: &str) -> Option<usize> {
    line.chars()
        .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
}

fn is_blank(line: &str) -> bool {
    first_non_space(line).is_none()
}

/// `line[start, length]` on characters.
fn char_slice(line: &str, start: usize, length: usize) -> String {
    line.chars().skip(start).take(length).collect()
}

impl PrecedingFollowingAlignment {
    /// `aligned_with_something?(range)`.
    pub fn aligned_with_something(&mut self, ps: &ProcessedSource<'_>, range: Range) -> bool {
        self.aligned_with_adjacent_line(ps, range, Predicate::Token)
    }

    /// `aligned_with_operator?(range)`.
    pub fn aligned_with_operator(&mut self, ps: &ProcessedSource<'_>, range: Range) -> bool {
        self.aligned_with_adjacent_line(ps, range, Predicate::Operator)
    }

    /// `aligned_with_preceding_equals_operator(token)`; `token` is the
    /// token's range.
    pub fn aligned_with_preceding_equals_operator(
        &mut self,
        ps: &ProcessedSource<'_>,
        token: Range,
    ) -> Aligned {
        let line = ps.line_of(token.begin_pos);
        let lines: Vec<usize> = (1..=line).rev().collect();
        self.aligned_with_equals_sign(ps, token, &lines)
    }

    /// `aligned_with_subsequent_equals_operator(token)`.
    pub fn aligned_with_subsequent_equals_operator(
        &mut self,
        ps: &ProcessedSource<'_>,
        token: Range,
    ) -> Aligned {
        let line = ps.line_of(token.begin_pos);
        let lines: Vec<usize> = (line..=ps.lines().len()).collect();
        self.aligned_with_equals_sign(ps, token, &lines)
    }

    fn aligned_with_adjacent_line(
        &mut self,
        ps: &ProcessedSource<'_>,
        range: Range,
        predicate: Predicate,
    ) -> bool {
        let line = ps.line_of(range.begin_pos);
        // 0-based line indexes.
        let pre: Vec<usize> = (0..line.saturating_sub(1)).rev().collect();
        let post: Vec<usize> = (line..ps.lines().len()).collect();
        self.aligned_with_any_line_range(ps, &[pre, post], range, predicate)
    }

    fn aligned_with_any_line_range(
        &mut self,
        ps: &ProcessedSource<'_>,
        line_ranges: &[Vec<usize>],
        range: Range,
        predicate: Predicate,
    ) -> bool {
        if self.aligned_with_any_line(ps, line_ranges, range, None, predicate) {
            return true;
        }
        let base_indentation = first_non_space(ps.lines()[ps.line_of(range.begin_pos) - 1]);
        self.aligned_with_any_line(ps, line_ranges, range, base_indentation, predicate)
    }

    fn aligned_with_any_line(
        &mut self,
        ps: &ProcessedSource<'_>,
        line_ranges: &[Vec<usize>],
        range: Range,
        indent: Option<usize>,
        predicate: Predicate,
    ) -> bool {
        line_ranges
            .iter()
            .any(|line_nos| self.aligned_with_line(ps, line_nos, range, indent, predicate))
    }

    fn aligned_with_line(
        &mut self,
        ps: &ProcessedSource<'_>,
        line_nos: &[usize],
        range: Range,
        indent: Option<usize>,
        predicate: Predicate,
    ) -> bool {
        for &lineno in line_nos {
            if self.aligned_comment_lines(ps).contains(&(lineno + 1)) {
                continue;
            }
            let line = ps.lines()[lineno];
            let Some(index) = first_non_space(line) else {
                continue;
            };
            if let Some(indent) = indent {
                if index < indent {
                    break;
                }
                if index > indent {
                    continue;
                }
            }
            return match predicate {
                Predicate::Token => aligned_token(ps, range, line, lineno + 1),
                Predicate::Operator => aligned_operator(ps, range, line, lineno + 1),
            };
        }
        false
    }

    pub fn aligned_comment_lines(&mut self, ps: &ProcessedSource<'_>) -> &HashSet<usize> {
        self.aligned_comment_lines.get_or_insert_with(|| {
            ps.comments()
                .iter()
                .filter(|c| begins_its_line(ps, c.range))
                .map(|c| ps.line_of(c.range.begin_pos))
                .collect()
        })
    }

    fn aligned_with_equals_sign(
        &mut self,
        ps: &ProcessedSource<'_>,
        token: Range,
        line_range: &[usize],
    ) -> Aligned {
        let token_line_indent = ps.line_indentation(ps.line_of(token.begin_pos));
        let assignment_lines = self.relevant_assignment_lines(ps, line_range);
        let Some(&relevant_line_number) = assignment_lines.get(1) else {
            return Aligned::None;
        };
        let relevant_indent = ps.line_indentation(relevant_line_number);
        if relevant_indent < token_line_indent {
            return Aligned::None;
        }
        if ps.lines().get(relevant_line_number - 1).is_none() {
            return Aligned::None;
        }
        if aligned_equals_operator(ps, token, relevant_line_number) {
            Aligned::Yes
        } else {
            Aligned::No
        }
    }

    fn assignment_lines(&mut self, ps: &ProcessedSource<'_>) -> &HashSet<usize> {
        self.assignment_lines
            .get_or_insert_with(|| assignment_tokens(ps).iter().map(|t| t.line(ps)).collect())
    }

    pub fn relevant_assignment_lines(
        &mut self,
        ps: &ProcessedSource<'_>,
        line_range: &[usize],
    ) -> Vec<usize> {
        let assignment_lines = self.assignment_lines(ps).clone();
        let boundary = self.interrupting_operator_lines(ps).clone();
        relevant_lines(ps, line_range, &boundary, |line| {
            assignment_lines.contains(&line)
        })
    }

    fn interrupting_operator_lines(&mut self, ps: &ProcessedSource<'_>) -> &HashSet<usize> {
        if self.interrupting_operator_lines.is_none() {
            let assignment_lines = self.assignment_lines(ps).clone();
            let lines = ps
                .tokens()
                .iter()
                .filter(|t| ASSIGNMENT_OR_COMPARISON_TOKENS.contains(&t.ty))
                .map(|t| t.line(ps))
                .filter(|line| !assignment_lines.contains(line))
                .collect();
            self.interrupting_operator_lines = Some(lines);
        }
        self.interrupting_operator_lines.as_ref().unwrap()
    }

    /// `alignment_lines(line_number)`.
    pub fn alignment_lines(&mut self, ps: &ProcessedSource<'_>, line_number: usize) -> Vec<usize> {
        if let Some(lines) = self.alignment_lines_by_line.get(&line_number) {
            return lines.clone();
        }
        let mut lines: Vec<usize> = self
            .alignment_line_ranges(ps, line_number)
            .into_iter()
            .flatten()
            .collect();
        lines.sort_unstable();
        lines.dedup();
        for &line in &lines {
            self.alignment_lines_by_line.insert(line, lines.clone());
        }
        lines
    }

    fn alignment_line_ranges(
        &mut self,
        ps: &ProcessedSource<'_>,
        line_number: usize,
    ) -> [Vec<usize>; 2] {
        let last_line = ps.lines().len();
        let comments = self.aligned_comment_lines(ps).clone();
        let boundary = self.definition_boundary_lines(ps).clone();
        let down: Vec<usize> = (1..=line_number).rev().collect();
        let up: Vec<usize> = (line_number..=last_line).collect();
        [
            relevant_lines(ps, &down, &boundary, |line| !comments.contains(&line)),
            relevant_lines(ps, &up, &boundary, |line| !comments.contains(&line)),
        ]
    }

    pub fn definition_boundary_lines(&mut self, ps: &ProcessedSource<'_>) -> &HashSet<usize> {
        self.definition_boundary_lines.get_or_insert_with(|| {
            let mut lines = HashSet::new();
            if let Some(ast) = ps.ast() {
                for node in ast.each_node().filter(|n| n.is_any_def()) {
                    lines.insert(node.first_line());
                    lines.insert(node.last_line());
                }
            }
            lines
        })
    }
}

fn aligned_token(ps: &ProcessedSource<'_>, range: Range, line: &str, lineno: usize) -> bool {
    aligned_words(ps, range, line) || aligned_equals_operator(ps, range, lineno)
}

fn aligned_operator(ps: &ProcessedSource<'_>, range: Range, line: &str, lineno: usize) -> bool {
    aligned_identical(ps, range, line) || aligned_equals_operator(ps, range, lineno)
}

fn aligned_words(ps: &ProcessedSource<'_>, range: Range, line: &str) -> bool {
    let left_edge = ps.column_of(range.begin_pos);
    if left_edge >= 1 {
        let pair: Vec<char> = line.chars().skip(left_edge - 1).take(2).collect();
        if pair.len() == 2 && pair[0].is_ascii_whitespace() && !pair[1].is_ascii_whitespace() {
            return true;
        }
    }
    let token = ps.slice(range);
    token == char_slice(line, left_edge, token.chars().count())
}

/// `aligned_equals_operator?(range, lineno)`.
pub fn aligned_equals_operator(ps: &ProcessedSource<'_>, range: Range, lineno: usize) -> bool {
    if lineno == 0 || lineno > ps.lines().len() {
        return false;
    }
    let line_range = ps.line_range(lineno);
    let operator_token = ps
        .tokens_within(line_range)
        .iter()
        .find(|t| ASSIGNMENT_OR_COMPARISON_TOKENS.contains(&t.ty));
    aligned_with_preceding_equals(ps, range, operator_token)
        || aligned_with_append_operator(ps, range, operator_token)
}

fn last_column(ps: &ProcessedSource<'_>, range: Range) -> usize {
    ps.column_of(range.end_pos)
}

fn aligned_with_preceding_equals(
    ps: &ProcessedSource<'_>,
    range: Range,
    token: Option<&Token>,
) -> bool {
    let Some(token) = token else {
        return false;
    };
    ps.slice(range).ends_with('=') && last_column(ps, range) == last_column(ps, token.range)
}

fn aligned_with_append_operator(
    ps: &ProcessedSource<'_>,
    range: Range,
    token: Option<&Token>,
) -> bool {
    let Some(token) = token else {
        return false;
    };
    let source = ps.slice(range);
    ((source == "<<" && token.is_equal_sign()) || (source.ends_with('=') && token.ty == "tLSHFT"))
        && last_column(ps, range) == last_column(ps, token.range)
}

fn aligned_identical(ps: &ProcessedSource<'_>, range: Range, line: &str) -> bool {
    let source = ps.slice(range);
    source == char_slice(line, ps.column_of(range.begin_pos), source.chars().count())
}

pub fn assignment_tokens<'p>(ps: &'p ProcessedSource<'_>) -> Vec<&'p Token> {
    let tokens: Vec<&Token> = ps.tokens().iter().filter(|t| t.is_equal_sign()).collect();
    let tokens = remove_equals_in_def(ps, tokens);
    let mut seen = HashSet::new();
    tokens
        .into_iter()
        .filter(|t| seen.insert(t.line(ps)))
        .collect()
}

/// `relevant_lines(line_range, boundary_lines)`.
fn relevant_lines(
    ps: &ProcessedSource<'_>,
    line_range: &[usize],
    boundary_lines: &HashSet<usize>,
    mut predicate: impl FnMut(usize) -> bool,
) -> Vec<usize> {
    let mut result = Vec::new();
    let Some(&original_line) = line_range.first() else {
        return result;
    };
    let original_line_indent = ps.line_indentation(original_line);
    let mut relevant_line_indent_at_level = true;
    for &line_number in line_range {
        let current_line_indent = ps.line_indentation(line_number);
        let blank_line = ps.lines().get(line_number - 1).is_none_or(|l| is_blank(l));
        if (line_number != original_line && boundary_lines.contains(&line_number))
            || (current_line_indent < original_line_indent && !blank_line)
            || (relevant_line_indent_at_level && blank_line)
        {
            break;
        }
        if predicate(line_number) && current_line_indent == original_line_indent {
            result.push(line_number);
        }
        if !blank_line {
            relevant_line_indent_at_level = current_line_indent == original_line_indent;
        }
    }
    result
}

fn remove_equals_in_def<'t>(
    ps: &ProcessedSource<'_>,
    asgn_tokens: Vec<&'t Token>,
) -> Vec<&'t Token> {
    let Some(ast) = ps.ast() else {
        return asgn_tokens;
    };
    let eqls_to_ignore: Vec<usize> = ast
        .each_node()
        .filter_map(|node| match node.ty() {
            NodeType::Def => {
                if node.is_endless() {
                    node.loc().assignment.map(|r| r.begin_pos)
                } else {
                    None
                }
            }
            NodeType::Optarg => node.loc().operator.map(|r| r.begin_pos),
            _ => None,
        })
        .collect();
    asgn_tokens
        .into_iter()
        .filter(|t| !eqls_to_ignore.contains(&t.begin_pos()))
        .collect()
}
