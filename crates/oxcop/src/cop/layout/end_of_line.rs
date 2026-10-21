//! Port of `lib/rubocop/cop/layout/end_of_line.rb`.

use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};

const MSG_DETECTED: &str = "Carriage return character detected.";
const MSG_MISSING: &str = "Carriage return character missing.";

#[derive(Default)]
pub struct EndOfLine;

impl Cop for EndOfLine {
    fn name(&self) -> &'static str {
        "Layout/EndOfLine"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let last_line = match ps.tokens().last() {
            Some(last_token) => last_token.line(ps),
            None => ps.lines().len(),
        };
        let style = style(cx);
        for (index, line) in ps.raw_source().split_inclusive('\n').enumerate() {
            if index >= last_line {
                break;
            }
            let Some(msg) = offense_message(style, line) else {
                continue;
            };
            if style == "crlf" && index == last_line - 1 && !line.ends_with('\n') {
                continue;
            }
            let range = source_range(ps, index + 1, 0, line.chars().count());
            cx.add_offense(range, msg);
            break;
        }
    }
}

fn offense_message(style: &str, line: &str) -> Option<&'static str> {
    let effective_style = if style == "native" {
        if cfg!(windows) { "crlf" } else { "lf" }
    } else {
        style
    };
    match effective_style {
        "lf" => (line.ends_with('\r') || line.ends_with("\r\n")).then_some(MSG_DETECTED),
        _ => (!line.ends_with("\r\n")).then_some(MSG_MISSING),
    }
}
