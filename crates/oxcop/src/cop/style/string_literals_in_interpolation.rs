//! Port of `lib/rubocop/cop/style/string_literals_in_interpolation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::string_literal_corrector;
use crate::cop::mixin::configurable_enforced_style::{
    correct_style_detected, opposite_style_detected, style,
};
use crate::cop::style::string_literals::check_style;
use crate::cop::mixin::string_help::{inside_interpolation, StringHelp};
use crate::cop::mixin::string_literals_help::wrong_quotes;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct StringLiteralsInInterpolation {
    string_help: StringHelp,
}

impl Cop for StringLiteralsInInterpolation {
    fn name(&self) -> &'static str {
        "Style/StringLiteralsInInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    // `on_regexp` is overridden to do nothing.
    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = self.string_help.candidate(node) else {
            return;
        };
        check_style(cx);
        if !(inside_interpolation(node) && wrong_quotes(cx, node.source())) {
            correct_style_detected(cx, "EnforcedStyle");
            return;
        }
        let style = style(cx);
        // `style.to_s.sub(/_(.*)s/, '-\1d')`.
        let kind = match style.split_once('_') {
            Some((head, tail)) => match tail.rfind('s') {
                Some(i) => format!("{head}-{}d{}", &tail[..i], &tail[i + 1..]),
                None => style.to_owned(),
            },
            None => style.to_owned(),
        };
        let message = format!("Prefer {kind} strings inside interpolations.");
        let mut ran = false;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            ran = true;
            string_literal_corrector::correct(corrector, node, style);
        });
        if ran {
            opposite_style_detected(cx, "EnforcedStyle");
        }
    }
}
