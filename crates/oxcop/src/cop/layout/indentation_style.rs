//! Port of `lib/rubocop/cop/layout/indentation_style.rb`.

use serde_yaml_ng::Value;

use crate::ast::NodeType;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

#[derive(Default)]
pub struct IndentationStyle;

impl Cop for IndentationStyle {
    fn name(&self) -> &'static str {
        "Layout/IndentationStyle"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let spaces = style(cx) == "spaces";
        let width = configured_indentation_width(cx);
        let message = format!(
            "{} detected in indentation.",
            if spaces { "Tab" } else { "Space" }
        );
        let mut str_ranges: Option<Vec<Range>> = None;
        for (index, line) in ps.lines().iter().enumerate() {
            let Some(range) = find_offense(ps, spaces, line, index + 1) else {
                continue;
            };
            let ranges = str_ranges.get_or_insert_with(|| string_literal_ranges(ps));
            if ranges.iter().any(|r| r.contains(range)) {
                continue;
            }
            cx.add_offense_with_correction(range, &message, |corrector| {
                let source = ps.slice(range);
                if source.contains('\t') {
                    corrector.replace(range, source.replace('\t', &" ".repeat(width)));
                } else {
                    let leading = source
                        .chars()
                        .take_while(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c'))
                        .count();
                    let tabs = "\t".repeat(leading.checked_div(width).unwrap_or(0));
                    let rest: String = source.chars().skip(leading).collect();
                    let replacement = if leading == 0 { rest } else { tabs + &rest };
                    corrector.replace(range, replacement);
                }
            });
        }
    }
}

/// `Alignment#configured_indentation_width`.
fn configured_indentation_width(cx: &Investigation<'_, '_>) -> usize {
    cx.cop_config_value("IndentationWidth")
        .and_then(Value::as_u64)
        .or_else(|| {
            cx.config
                .for_cop("Layout/IndentationWidth")
                .get("Width")
                .and_then(Value::as_u64)
        })
        .unwrap_or(2) as usize
}

fn find_offense(ps: &ProcessedSource<'_>, spaces: bool, line: &str, lineno: usize) -> Option<Range> {
    let re = if spaces {
        regex!(r"\A\s*\t+")
    } else {
        regex!(r"\A\s* +")
    };
    let (start, end) = re.find(line)?;
    let begin = line[..start].chars().count();
    let length = line[start..end].chars().count();
    Some(source_range(ps, lineno, begin, length))
}

fn string_literal_ranges(ps: &ProcessedSource<'_>) -> Vec<Range> {
    let Some(ast) = ps.ast() else {
        return Vec::new();
    };
    ast.each_node()
        .filter(|n| n.is_any(&[NodeType::Str, NodeType::Dstr]))
        .filter_map(|str| {
            if str.is_heredoc() {
                str.loc().heredoc_body
            } else if str.loc().begin.is_some() {
                Some(str.source_range())
            } else {
                None
            }
        })
        .collect()
}
