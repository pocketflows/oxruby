//! Port of `lib/rubocop/cop/layout/space_around_equals_in_parameter_default.rb`.

use crate::ast::token::Token;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct SpaceAroundEqualsInParameterDefault;

impl Cop for SpaceAroundEqualsInParameterDefault {
    fn name(&self) -> &'static str {
        "Layout/SpaceAroundEqualsInParameterDefault"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Optarg]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let tokens = cx.processed_source.tokens_within(node.source_range());
        let [arg, equals, value, ..] = tokens else {
            return;
        };
        check_optarg(cx, arg, equals, value);
    }
}

fn check_optarg(cx: &mut Investigation<'_, '_>, arg: &Token, equals: &Token, value: &Token) {
    let ps = cx.processed_source;
    let space_on_both_sides = arg.is_space_after(ps) && equals.is_space_after(ps);
    let no_surrounding_space = !arg.is_space_after(ps) && !equals.is_space_after(ps);
    let style = style(cx);
    if (style == "space" && space_on_both_sides) || (style == "no_space" && no_surrounding_space)
    {
        return;
    }
    let range = range_between(arg.end_pos(), value.begin_pos());
    let message = format!(
        "Surrounding space {} in default value assignment.",
        if style == "space" { "missing" } else { "detected" }
    );
    let source = cx.source(range);
    cx.add_offense_with_correction(range, &message, |corrector| {
        let rest = regex!(r"=\s*(\S+)")
            .captures(source)
            .and_then(|m| m.at(1))
            .unwrap_or("");
        let replacement = if style == "space" { " = " } else { "=" };
        corrector.replace(range, format!("{replacement}{rest}"));
    });
}
