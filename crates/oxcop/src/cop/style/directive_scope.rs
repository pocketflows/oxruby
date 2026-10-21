//! Port of `lib/rubocop/cop/style/directive_scope.rb`.

use crate::comment_config::{DirectiveRange, DirectiveScopes, LINE_MAX, LINE_MIN};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Corrector, Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::{Comment, ProcessedSource};

#[derive(Default)]
pub struct DirectiveScope;

impl Cop for DirectiveScope {
    fn name(&self) -> &'static str {
        "Style/DirectiveScope"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let mut scopes = DirectiveScopes::new(ps, cx.config);
        let comments: Vec<Comment<'_>> = ps.comments().to_vec();
        for comment in &comments {
            let directive = directive_of(ps, comment);
            if !scopes.comment_only_line(directive.line_number) {
                continue;
            }
            if directive.disabled() && !directive.disable_next() {
                check_pair(cx, &mut scopes, &directive);
            } else if directive.push() && !directive.signed_args().is_empty() {
                check_push_pop(cx, &mut scopes, &comments, &directive);
            } else if directive.enabled() && !directive.enable_next() && !directive.all_cops() {
                check_enable_pair(cx, &mut scopes, &directive);
            }
        }
    }
}

fn directive_of<'s>(ps: &ProcessedSource<'_>, comment: &Comment<'s>) -> DirectiveComment<'s> {
    DirectiveComment::new(
        comment.text,
        comment.range,
        ps.line_of(comment.range.begin_pos) as i64,
    )
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

fn mode<'d>(directive: &'d DirectiveComment<'_>) -> &'d str {
    directive.mode.as_deref().unwrap_or("")
}

fn check_pair(
    cx: &mut Investigation<'_, '_>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
) {
    let Some(enable) = single_statement_closing_directive(cx, scopes, directive) else {
        return;
    };
    if !enable.enabled() || sorted(enable.raw_cop_names()) != sorted(directive.raw_cop_names()) {
        return;
    }
    let mode = mode(directive).to_owned();
    let message = format!(
        "Use `{mode}-next` instead of a `{mode}`/`enable` pair around a single statement."
    );
    let ps = cx.processed_source;
    let text = directive.comment_text;
    let range = directive.comment_range;
    let enable_range = enable.comment_range;
    cx.add_offense_with_correction(range, &message, |corrector| {
        let re = crate::cop::mixin::ruby_regex(&format!(r"\b{mode}\b"));
        let replacement = match re.as_ref().and_then(|r| r.find(text)) {
            Some((start, end)) => format!("{}{mode}-next{}", &text[..start], &text[end..]),
            None => text.to_owned(),
        };
        corrector.replace(range, replacement);
        remove_line(ps, corrector, enable_range);
    });
}

fn check_push_pop(
    cx: &mut Investigation<'_, '_>,
    scopes: &mut DirectiveScopes<'_, '_>,
    comments: &[Comment<'_>],
    directive: &DirectiveComment<'_>,
) {
    let ps = cx.processed_source;
    let Some(pop_line) = balancing_pop_line(ps, comments, directive) else {
        return;
    };
    if !scopes.comment_only_line(pop_line) || !wraps_single_statement(scopes, directive, pop_line) {
        return;
    }
    let Some(pop_comment) = ps.comment_at_line(pop_line as usize).copied() else {
        return;
    };
    let replacement = push_replacement(directive);
    let message = format!(
        "Use `{}` instead of `push`/`pop` around a single statement.",
        replacement_mode(directive)
    );
    let range = directive.comment_range;
    cx.add_offense_with_correction(range, &message, |corrector| {
        corrector.replace(range, replacement);
        remove_line(ps, corrector, pop_comment.range);
    });
}

fn balancing_pop_line(
    ps: &ProcessedSource<'_>,
    comments: &[Comment<'_>],
    push_directive: &DirectiveComment<'_>,
) -> Option<i64> {
    let mut depth = 0;
    for comment in comments {
        let directive = directive_of(ps, comment);
        if directive.line_number <= push_directive.line_number {
            continue;
        }
        if directive.push() {
            depth += 1;
        } else if directive.pop() {
            if depth == 0 {
                return Some(directive.line_number);
            }
            depth -= 1;
        }
    }
    None
}

fn signed(directive: &DirectiveComment<'_>, op: char) -> Vec<String> {
    directive
        .signed_args()
        .into_iter()
        .find(|(o, _)| *o == op)
        .map(|(_, names)| names)
        .unwrap_or_default()
}

fn replacement_mode(directive: &DirectiveComment<'_>) -> &'static str {
    let mut ops: Vec<char> = directive.signed_args().iter().map(|(o, _)| *o).collect();
    ops.sort();
    match ops.as_slice() {
        ['-'] => "disable-next",
        ['+'] => "enable-next",
        _ => "next",
    }
}

fn push_replacement(directive: &DirectiveComment<'_>) -> String {
    let text = match replacement_mode(directive) {
        "disable-next" => format!("# rubocop:disable-next {}", signed(directive, '-').join(", ")),
        "enable-next" => format!("# rubocop:enable-next {}", signed(directive, '+').join(", ")),
        _ => format!("# rubocop:next {}", directive.cops.as_deref().unwrap_or("")),
    };
    match directive.reason() {
        Some(reason) => format!("{text} -- {reason}"),
        None => text,
    }
}

fn check_enable_pair(
    cx: &mut Investigation<'_, '_>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
) {
    let Some(closing) = enable_pair_closing(cx, scopes, directive) else {
        return;
    };
    let ps = cx.processed_source;
    let range = directive.comment_range;
    let text = directive.comment_text;
    let closing_range = closing.comment_range;
    cx.add_offense_with_correction(
        range,
        "Use `enable-next` instead of an `enable`/`disable` pair around a single statement.",
        |corrector| {
            let replacement = match crate::regex!(r"\benable\b").find(text) {
                Some((start, end)) => format!("{}enable-next{}", &text[..start], &text[end..]),
                None => text.to_owned(),
            };
            corrector.replace(range, replacement);
            remove_line(ps, corrector, closing_range);
        },
    );
}

fn enable_pair_closing<'s>(
    cx: &Investigation<'_, 's>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
) -> Option<DirectiveComment<'s>> {
    let (begin, end) = scopes.statement_scope_after(directive.line_number)?;
    if begin != directive.line_number + 1 {
        return None;
    }
    let closing = re_disable_below(cx, scopes, directive, end + 1)?;
    closed_open_disables(cx, scopes, directive, &closing).then_some(closing)
}

fn re_disable_below<'s>(
    cx: &Investigation<'_, 's>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
    line: i64,
) -> Option<DirectiveComment<'s>> {
    if !scopes.comment_only_line(line) {
        return None;
    }
    let ps = cx.processed_source;
    let comment = *ps.comment_at_line(usize::try_from(line).ok()?)?;
    let closing = directive_of(ps, &comment);
    if !closing.disabled() || closing.disable_next() {
        return None;
    }
    if sorted(closing.raw_cop_names()) != sorted(directive.raw_cop_names()) {
        return None;
    }
    Some(closing)
}

fn directive_comment_range(
    cx: &Investigation<'_, '_>,
    range: &DirectiveRange,
) -> Option<crate::processed_source::Range> {
    range
        .directive
        .and_then(|i| cx.comment_config().directives.get(i))
        .map(|d| d.comment_range)
}

fn closed_open_disables(
    cx: &Investigation<'_, '_>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
    closing: &DirectiveComment<'_>,
) -> bool {
    let registry = scopes.registry();
    directive.cop_names(registry).iter().all(|cop| {
        let Some(ranges) = cx
            .comment_config()
            .cop_disabled_line_ranges
            .get(&scopes.qualified_cop_name(cop))
        else {
            return false;
        };
        ranges.iter().any(|r| r.end == directive.line_number)
            && ranges
                .iter()
                .any(|r| directive_comment_range(cx, r) == Some(closing.comment_range))
    })
}

fn single_statement_closing_directive<'s>(
    cx: &Investigation<'_, 's>,
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
) -> Option<DirectiveComment<'s>> {
    let closing_line = single_closing_line(cx, directive)?;
    let ps = cx.processed_source;
    let closing_comment = *ps.comment_at_line(usize::try_from(closing_line).ok()?)?;
    if !wraps_single_statement(scopes, directive, closing_line) {
        return None;
    }
    Some(directive_of(ps, &closing_comment))
}

fn single_closing_line(cx: &Investigation<'_, '_>, directive: &DirectiveComment<'_>) -> Option<i64> {
    let mut ends: Vec<i64> = Vec::new();
    for ranges in cx.comment_config().cop_disabled_line_ranges.values() {
        for range in ranges {
            if directive_comment_range(cx, range) == Some(directive.comment_range)
                && !ends.contains(&range.end)
            {
                ends.push(range.end);
            }
        }
    }
    match ends.as_slice() {
        [end] if *end != LINE_MAX && *end != LINE_MIN => Some(*end),
        _ => None,
    }
}

fn wraps_single_statement(
    scopes: &mut DirectiveScopes<'_, '_>,
    directive: &DirectiveComment<'_>,
    closing_line: i64,
) -> bool {
    scopes
        .statement_scope_after(directive.line_number)
        .is_some_and(|(begin, end)| begin == directive.line_number + 1 && end + 1 == closing_line)
}

fn remove_line(ps: &ProcessedSource<'_>, corrector: &mut Corrector, range: crate::processed_source::Range) {
    corrector.remove(range_by_whole_lines(ps, range, true));
}
