//! Port of `lib/rubocop/cop/lint/redundant_cop_enable_directive.rb`, with
//! `CommentConfig#extra_enabled_comments` (and its helpers).

use std::collections::{BTreeSet, HashMap};

use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_between, range_with_surrounding_space,
};
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::{Comment, Range};

const MSG: &str = "Unnecessary enabling of %<cop>s.";
const MSG_ORPHAN_POP: &str = "Unnecessary `rubocop:pop` without a matching `rubocop:push`.";
const TRAILING_COMMENT_MARKER: &str = "--";

#[derive(Default)]
pub struct RedundantCopEnableDirective;

impl Cop for RedundantCopEnableDirective {
    fn name(&self) -> &'static str {
        "Lint/RedundantCopEnableDirective"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.raw_source().trim().is_empty() {
            return;
        }
        let source = ps.raw_source();
        if source.contains("enable") {
            check_extra_enables(cx);
        }
        if source.contains("pop") {
            check_orphan_pops(cx);
        }
    }
}

fn directive<'s>(cx: &Investigation<'_, 's>, comment: &Comment<'s>) -> DirectiveComment<'s> {
    DirectiveComment::new(
        comment.text,
        comment.range,
        cx.processed_source.line_of(comment.range.begin_pos) as i64,
    )
}

fn check_extra_enables(cx: &mut Investigation<'_, '_>) {
    for (comment, cop_names) in extra_enabled_comments(cx) {
        register_offense(cx, &comment, &cop_names);
    }
}

/// `CommentConfig#extra_enabled_comments`: each enable directive's cop
/// names that weren't disabled before it.
fn extra_enabled_comments<'s>(cx: &Investigation<'_, 's>) -> Vec<(Comment<'s>, Vec<String>)> {
    let ps = cx.processed_source;
    let registry = cx.config.known_cops();
    let mut names: HashMap<String, i64> = HashMap::new();
    for name in &cx.comment_config().disabled_names {
        *names.entry(name.clone()).or_default() += 1;
    }
    let non_comment_lines: BTreeSet<usize> = ps
        .tokens()
        .iter()
        .filter(|t| t.ty != "tCOMMENT")
        .map(|t| ps.line_of(t.range.begin_pos))
        .collect();

    let mut extras: Vec<(Comment<'s>, Vec<String>)> = Vec::new();
    let mut push_extra = |comment: &Comment<'s>, name: String| {
        match extras.iter_mut().find(|(c, _)| c.range == comment.range) {
            Some((_, list)) => list.push(name),
            None => extras.push((*comment, vec![name])),
        }
    };
    for comment in ps.comments() {
        let directive = directive(cx, comment);
        if !directive.has_match() {
            continue;
        }
        if non_comment_lines.contains(&ps.line_of(comment.range.begin_pos)) {
            continue;
        }
        if self_closing_directive(&directive) {
            continue;
        }
        if directive.enabled_all() {
            // handle_enable_all
            let mut enabled_cops = 0;
            for counter in names.values_mut() {
                if *counter > 0 {
                    *counter -= 1;
                    enabled_cops += 1;
                }
            }
            if enabled_cops == 0 {
                push_extra(comment, "all".to_owned());
            }
        } else {
            // handle_switch
            for name in directive.cop_names(registry) {
                let counter = names.entry(name.clone()).or_default();
                if directive.disabled() {
                    *counter += 1;
                } else if *counter > 0 {
                    *counter -= 1;
                } else {
                    push_extra(comment, name);
                }
            }
        }
    }
    extras
}

/// `CommentConfig#self_closing_directive?`.
fn self_closing_directive(directive: &DirectiveComment<'_>) -> bool {
    directive.push()
        || directive.pop()
        || directive.disable_next()
        || directive.next()
        || directive.enable_next()
}

fn check_orphan_pops(cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    let mut push_depth = 0usize;
    for comment in ps.comments() {
        let directive = directive(cx, comment);
        if directive.push() {
            push_depth += 1;
        } else if directive.pop() {
            if push_depth == 0 {
                register_orphan_pop(cx, &directive);
            } else {
                push_depth -= 1;
            }
        }
    }
}

fn register_orphan_pop(cx: &mut Investigation<'_, '_>, directive: &DirectiveComment<'_>) {
    let Some(range) = directive.range() else {
        return;
    };
    let src = cx.processed_source.raw_source();
    let removal = range_with_surrounding_space(src, range, SurroundingSpace::side(Side::Right));
    cx.add_offense_with_correction(range, MSG_ORPHAN_POP, |corrector| corrector.remove(removal));
}

fn register_offense(cx: &mut Investigation<'_, '_>, comment: &Comment<'_>, cop_names: &[String]) {
    let directive = directive(cx, comment);
    for name in cop_names {
        let name = if department(cx, &directive, name) {
            name.split('/').next().unwrap_or(name).to_owned()
        } else {
            name.clone()
        };
        let Some(range) = range_of_offense(comment, &name) else {
            continue;
        };
        let message = MSG.replace("%<cop>s", all_or_name(&name));
        let removal = removal_range(cx, &directive, comment, cop_names, &name);
        cx.add_offense_with_correction(range, &message, |corrector| {
            if let Some(removal) = removal {
                corrector.remove(removal);
            }
        });
    }
}

fn removal_range(
    cx: &Investigation<'_, '_>,
    directive: &DirectiveComment<'_>,
    comment: &Comment<'_>,
    cop_names: &[String],
    name: &str,
) -> Option<Range> {
    if directive_match(cx, directive, cop_names) {
        let range = range_with_reason(directive, comment)?;
        let src = cx.processed_source.raw_source();
        Some(range_with_surrounding_space(
            src,
            range,
            SurroundingSpace::side(Side::Right),
        ))
    } else {
        range_with_comma(comment, name)
    }
}

/// `DirectiveComment#match?(cop_names)`.
fn directive_match(
    cx: &Investigation<'_, '_>,
    directive: &DirectiveComment<'_>,
    cop_names: &[String],
) -> bool {
    let sorted_uniq = |names: &[String]| -> Vec<String> {
        names
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    sorted_uniq(&directive.parsed_cop_names(cx.config.known_cops())) == sorted_uniq(cop_names)
}

/// `DirectiveComment#range_with_reason`: a `--` reason goes with the
/// directive.
fn range_with_reason(directive: &DirectiveComment<'_>, comment: &Comment<'_>) -> Option<Range> {
    let range = directive.range()?;
    let trailing_start = range.end_pos - comment.range.begin_pos;
    let trailing = comment.text.get(trailing_start..).unwrap_or_default();
    if trailing.trim_start().starts_with(TRAILING_COMMENT_MARKER) {
        Some(range_between(range.begin_pos, comment.range.end_pos))
    } else {
        Some(range)
    }
}

fn range_of_offense(comment: &Comment<'_>, name: &str) -> Option<Range> {
    let start = comment.range.begin_pos + cop_name_indention(comment, name)?;
    Some(range_between(start, start + name.len()))
}

/// `comment.text.index(/#{Regexp.escape(name)}(?!\w)/)`.
fn cop_name_indention(comment: &Comment<'_>, name: &str) -> Option<usize> {
    let text = comment.text;
    let mut from = 0;
    while let Some(i) = text.get(from..)?.find(name) {
        let at = from + i;
        let next = text[at + name.len()..].chars().next();
        if !next.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

fn range_with_comma(comment: &Comment<'_>, name: &str) -> Option<Range> {
    let source = comment.text.as_bytes();
    let begin = cop_name_indention(comment, name)? as isize;
    let end = begin + name.len() as isize;
    let begin = reposition(source, begin, -1);
    let end = reposition(source, end, 1);
    Some(range_to_remove(begin, end, comment))
}

/// `SurroundingSpace#reposition` over the comment's source (Ruby's negative
/// indexes count from the end).
fn reposition(src: &[u8], mut pos: isize, step: isize) -> isize {
    let offset = if step == -1 { -1 } else { 0 };
    let at = |i: isize| -> Option<u8> {
        let i = if i < 0 { src.len() as isize + i } else { i };
        usize::try_from(i).ok().and_then(|i| src.get(i).copied())
    };
    while matches!(at(pos + offset), Some(b' ' | b'\t')) {
        pos += step;
    }
    pos.max(0)
}

fn range_to_remove(begin: isize, end: isize, comment: &Comment<'_>) -> Range {
    let start = comment.range.begin_pos;
    let source = comment.text.as_bytes();
    let byte = |i: isize| -> Option<u8> {
        let i = if i < 0 { source.len() as isize + i } else { i };
        usize::try_from(i).ok().and_then(|i| source.get(i).copied())
    };
    let (begin_u, end_u) = (begin as usize, end as usize);
    if byte(begin - 1) == Some(b',') {
        // range_with_comma_before
        range_between(start + begin_u - 1, start + end_u)
    } else if byte(end) == Some(b',') {
        // range_with_comma_after
        let begin_u = if byte(end + 1) != Some(b' ') {
            begin_u + 1
        } else {
            begin_u
        };
        range_between(start + begin_u, start + end_u + 1)
    } else {
        range_between(start, comment.range.end_pos)
    }
}

fn all_or_name(name: &str) -> &str {
    if name == "all" { "all cops" } else { name }
}

/// `department?(directive, name)`.
fn department(cx: &Investigation<'_, '_>, directive: &DirectiveComment<'_>, name: &str) -> bool {
    let registry = cx.config.known_cops();
    let in_department = directive
        .department_names(registry)
        .iter()
        .any(|department| name.starts_with(department.as_str()));
    let overridden = in_department && directive.raw_cop_names().iter().any(|n| n == name);
    in_department && !overridden
}
