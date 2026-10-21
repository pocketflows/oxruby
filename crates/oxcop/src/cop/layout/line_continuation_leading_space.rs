//! Port of `lib/rubocop/cop/layout/line_continuation_leading_space.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;
use crate::regex;

/// `LINE_1_ENDING`.
fn line_1_ending() -> &'static onig::Regex {
    regex!(r#"['"]\s*\\\n"#)
}

/// `LINE_2_BEGINNING`.
fn line_2_beginning() -> &'static onig::Regex {
    regex!(r#"\A\s*['"]"#)
}

/// `LEADING_STYLE_OFFENSE`: `trailing_spaces` is group 1, `ending` group 2.
fn leading_style_offense() -> &'static onig::Regex {
    regex!(r#"(\s+)(['"]\s*\\\n)"#)
}

/// `TRAILING_STYLE_OFFENSE`: `beginning` is group 1, `leading_spaces` group 2.
fn trailing_style_offense() -> &'static onig::Regex {
    regex!(r#"(\A\s*['"])(\s+)"#)
}

#[derive(Default)]
pub struct LineContinuationLeadingSpace;

impl Cop for LineContinuationLeadingSpace {
    fn name(&self) -> &'static str {
        "Layout/LineContinuationLeadingSpace"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/StringLiterals"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.source().contains('\\') {
            return;
        }
        let ps = cx.processed_source;
        let mut end_of_first_line = ps.line_start(node.first_line());
        let lines = raw_lines(cx, node);
        for (offset, pair) in lines.windows(2).enumerate() {
            let (raw_line_one, raw_line_two) = (pair[0], pair[1]);
            let line_num = node.first_line() + offset;
            end_of_first_line += raw_line_one.len();
            if !continuation(raw_line_one, line_num, node) {
                continue;
            }
            investigate(cx, raw_line_one, raw_line_two, end_of_first_line);
        }
    }
}

fn raw_lines<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> Vec<&'s str> {
    cx.processed_source
        .raw_source()
        .split_inclusive('\n')
        .skip(node.first_line() - 1)
        .take(node.last_line() - node.first_line() + 1)
        .collect()
}

fn investigate(
    cx: &mut Investigation<'_, '_>,
    first_line: &str,
    second_line: &str,
    end_of_first_line: usize,
) {
    if enforced_style_leading(cx) {
        investigate_leading_style(cx, first_line, second_line, end_of_first_line);
    } else {
        investigate_trailing_style(cx, first_line, second_line, end_of_first_line);
    }
}

fn investigate_leading_style(
    cx: &mut Investigation<'_, '_>,
    first_line: &str,
    second_line: &str,
    end_of_first_line: usize,
) {
    let Some(matches) = leading_style_offense().captures(first_line) else {
        return;
    };
    let trailing_spaces = matches.at(1).unwrap().to_owned();
    let ending = matches.at(2).unwrap();
    let offense_range = leading_offense_range(end_of_first_line, &trailing_spaces, ending);
    let beginning_len = line_2_beginning()
        .find(second_line)
        .map_or(0, |(begin, end)| end - begin);
    let insert_pos = end_of_first_line + beginning_len;
    let message = message(cx);
    cx.add_offense_with_correction(offense_range, message, |corrector| {
        autocorrect(corrector, offense_range, insert_pos, &trailing_spaces);
    });
}

fn investigate_trailing_style(
    cx: &mut Investigation<'_, '_>,
    first_line: &str,
    second_line: &str,
    end_of_first_line: usize,
) {
    let Some(matches) = trailing_style_offense().captures(second_line) else {
        return;
    };
    let beginning = matches.at(1).unwrap();
    let leading_spaces = matches.at(2).unwrap().to_owned();
    let offense_range = trailing_offense_range(end_of_first_line, beginning, &leading_spaces);
    let ending_len = line_1_ending()
        .find(first_line)
        .map_or(0, |(begin, end)| end - begin);
    let insert_pos = end_of_first_line - ending_len;
    let message = message(cx);
    cx.add_offense_with_correction(offense_range, message, |corrector| {
        autocorrect(corrector, offense_range, insert_pos, &leading_spaces);
    });
}

fn continuation(line: &str, line_num: usize, node: Node<'_>) -> bool {
    if !line.ends_with("\\\n") {
        return false;
    }
    !node
        .child_nodes()
        .any(|c| (c.first_line()..c.last_line()).contains(&line_num) && c.is_multiline())
}

fn autocorrect(corrector: &mut Corrector, offense_range: Range, insert_pos: usize, spaces: &str) {
    corrector.remove(offense_range);
    corrector.replace(range_between(insert_pos, insert_pos), spaces);
}

fn leading_offense_range(end_of_first_line: usize, trailing_spaces: &str, ending: &str) -> Range {
    let end_pos = end_of_first_line - ending.len();
    let begin_pos = end_pos - trailing_spaces.len();
    range_between(begin_pos, end_pos)
}

fn trailing_offense_range(end_of_first_line: usize, beginning: &str, leading_spaces: &str) -> Range {
    let begin_pos = end_of_first_line + beginning.len();
    let end_pos = begin_pos + leading_spaces.len();
    range_between(begin_pos, end_pos)
}

fn message(cx: &Investigation<'_, '_>) -> &'static str {
    if enforced_style_leading(cx) {
        "Move trailing spaces to the start of the next line."
    } else {
        "Move leading spaces to the end of the previous line."
    }
}

fn enforced_style_leading(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("leading")
}
