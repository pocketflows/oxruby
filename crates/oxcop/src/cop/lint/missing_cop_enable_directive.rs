//! Port of `lib/rubocop/cop/lint/missing_cop_enable_directive.rb`.

use serde_yaml_ng::Value;

use crate::comment_config::{LINE_MAX, LINE_MIN};
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;

const MSG: &str = "Re-enable %<cop>s %<type>s with `%<directive>s` after disabling it.";
const MSG_BOUND: &str = "Re-enable %<cop>s %<type>s within %<max_range>s lines after disabling it.";

#[derive(Default)]
pub struct MissingCopEnableDirective;

impl Cop for MissingCopEnableDirective {
    fn name(&self) -> &'static str {
        "Lint/MissingCopEnableDirective"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let comment_config = cx.comment_config();
        let mut missing = Vec::new();
        // each_missing_enable
        for (cop, line_ranges) in &comment_config.cop_disabled_line_ranges {
            for line_range in line_ranges {
                let directive = line_range
                    .directive
                    .map(|index| &comment_config.directives[index]);
                if directive.is_some_and(|d| directive_comment(cx, d.comment_range).disable_next()) {
                    continue;
                }
                missing.push((cop.clone(), *line_range));
            }
        }
        missing.sort_by_key(|(_, range)| (range.begin, range.end));

        for (cop, line_range) in missing {
            if acceptable_range(cx, &cop, line_range.begin, line_range.end) {
                continue;
            }
            let Some(comment) = usize::try_from(line_range.begin)
                .ok()
                .and_then(|line| ps.comment_at_line(line))
            else {
                continue;
            };
            let message = message(cx, &cop, comment.range);
            cx.add_offense(comment.range, &message);
        }
    }
}

fn directive_comment<'s>(
    cx: &Investigation<'_, 's>,
    range: crate::processed_source::Range,
) -> DirectiveComment<'s> {
    let ps = cx.processed_source;
    DirectiveComment::new(cx.source(range), range, ps.line_of(range.begin_pos) as i64)
}

/// A line number as Ruby sees it: the open ends are ±infinity.
fn line(line: i64) -> f64 {
    match line {
        LINE_MIN => f64::NEG_INFINITY,
        LINE_MAX => f64::INFINITY,
        n => n as f64,
    }
}

fn acceptable_range(cx: &Investigation<'_, '_>, cop: &str, begin: i64, end: i64) -> bool {
    if line(end) - line(begin) < max_range(cx) + 2.0 {
        return true;
    }
    if !cx.config.known_cops().contains(cop) {
        return true;
    }
    expected_open_range(cx, cop, begin, end)
}

fn expected_open_range(cx: &Investigation<'_, '_>, cop: &str, begin: i64, end: i64) -> bool {
    if begin == LINE_MIN {
        return true;
    }
    // `processed_source.registry.enabled?(cop_class, config)`
    !cx.config.cop_enabled(cop) && end == LINE_MAX
}

fn max_range(cx: &Investigation<'_, '_>) -> f64 {
    match cx.cop_config_value("MaxRangeSize") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::INFINITY),
        _ => f64::INFINITY,
    }
}

fn max_range_text(cx: &Investigation<'_, '_>) -> String {
    match cx.cop_config_value("MaxRangeSize") {
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => n.to_string(),
        Some(Value::Number(n)) => format!("{:?}", n.as_f64().unwrap_or_default()),
        _ => "Infinity".to_owned(),
    }
}

fn message(cx: &Investigation<'_, '_>, cop: &str, comment: crate::processed_source::Range) -> String {
    let directive = directive_comment(cx, comment);
    let (cop, kind) = if department_enabled(cx, &directive, cop) {
        (cop.split('/').next().unwrap_or(cop), "department")
    } else {
        (cop, "cop")
    };
    if max_range(cx) == f64::INFINITY {
        let enabling = if directive.push() {
            "# rubocop:pop"
        } else {
            "# rubocop:enable"
        };
        MSG.replace("%<cop>s", cop)
            .replace("%<type>s", kind)
            .replace("%<directive>s", enabling)
    } else {
        MSG_BOUND
            .replace("%<cop>s", cop)
            .replace("%<type>s", kind)
            .replace("%<max_range>s", &max_range_text(cx))
    }
}

/// `DirectiveComment#in_directive_department?`, with the global registry.
fn department_enabled(cx: &Investigation<'_, '_>, directive: &DirectiveComment<'_>, cop: &str) -> bool {
    directive
        .department_names(cx.config.known_cops())
        .iter()
        .any(|department| cop.starts_with(department.as_str()))
}
