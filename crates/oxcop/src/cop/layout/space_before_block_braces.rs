//! Port of `lib/rubocop/cop/layout/space_before_block_braces.rb`.

use crate::ast::{Loc, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::{
    correct_style_detected, opposite_style_detected, style,
};
use crate::cop::mixin::range_help::{SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MISSING_MSG: &str = "Space missing to the left of {.";
const DETECTED_MSG: &str = "Space detected to the left of {.";

#[derive(Default)]
pub struct SpaceBeforeBlockBraces;

impl Cop for SpaceBeforeBlockBraces {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeBlockBraces"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/SymbolProc"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_keywords() {
            return;
        }
        if conflict_with_block_delimiters(cx, node) {
            return;
        }
        let left_brace = node.loc().begin.unwrap();
        let space_plus_brace = range_with_surrounding_space(
            cx.processed_source.buffer_source(),
            left_brace,
            SurroundingSpace::default(),
        );
        let used_style = if cx.source(space_plus_brace).starts_with('{') {
            "no_space"
        } else {
            "space"
        };
        if empty_braces(node.loc()) {
            check_empty(cx, left_brace, space_plus_brace, used_style);
        } else {
            check_non_empty(cx, left_brace, space_plus_brace, used_style);
        }
    }
}

fn check_empty(
    cx: &mut Investigation<'_, '_>,
    left_brace: Range,
    space_plus_brace: Range,
    used_style: &str,
) {
    let style_for_empty_braces = style_for_empty_braces(cx);
    if style_for_empty_braces == used_style {
        handle_different_styles_for_empty_braces(cx, used_style);
        return;
    } else if !cx.config_to_allow_offenses().contains_key("Enabled") {
        cx.config_to_allow_offenses()
            .insert("EnforcedStyleForEmptyBraces".into(), used_style.into());
    }
    let (range, msg) = if style_for_empty_braces == "space" {
        (left_brace, MISSING_MSG)
    } else {
        (
            range_between(space_plus_brace.begin_pos, left_brace.begin_pos),
            DETECTED_MSG,
        )
    };
    register(cx, range, msg, false);
}

fn handle_different_styles_for_empty_braces(cx: &mut Investigation<'_, '_>, used_style: &str) {
    let detected = cx
        .config_to_allow_offenses()
        .get("EnforcedStyleForEmptyBraces")
        .and_then(serde_yaml_ng::Value::as_str)
        .map(|s| s.trim_start_matches(':').to_owned());
    if detected.is_some_and(|detected| detected != used_style) {
        let config = cx.config_to_allow_offenses();
        config.clear();
        config.insert("Enabled".into(), false.into());
    }
}

fn check_non_empty(
    cx: &mut Investigation<'_, '_>,
    left_brace: Range,
    space_plus_brace: Range,
    used_style: &str,
) {
    if used_style == style(cx) {
        correct_style_detected(cx, "EnforcedStyle");
    } else if used_style == "space" {
        let space = range_between(space_plus_brace.begin_pos, left_brace.begin_pos);
        register(cx, space, DETECTED_MSG, true);
    } else {
        register(cx, left_brace, MISSING_MSG, true);
    }
}

/// `add_offense(range) { autocorrect; opposite_style_detected }`.
fn register(cx: &mut Investigation<'_, '_>, range: Range, msg: &str, detect: bool) {
    let source = cx.source(range);
    let line = cx.processed_source.line_of(range.begin_pos);
    let yields = !cx.has_offense_at(range) && cx.enabled_line(line);
    cx.add_offense_with_correction(range, msg, |corrector| {
        autocorrect(corrector, range, source);
    });
    if detect && yields {
        opposite_style_detected(cx, "EnforcedStyle");
    }
}

fn autocorrect(corrector: &mut Corrector, range: Range, source: &str) {
    if source
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
    {
        corrector.remove(range);
    } else {
        corrector.insert_before(range, " ");
    }
}

fn style_for_empty_braces<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    match cx.cop_config_value("EnforcedStyleForEmptyBraces") {
        Some(serde_yaml_ng::Value::String(s)) if s == "space" => "space",
        Some(serde_yaml_ng::Value::String(s)) if s == "no_space" => "no_space",
        None | Some(serde_yaml_ng::Value::Null) => style(cx),
        Some(_) => panic!("Unknown EnforcedStyleForEmptyBraces selected!"),
    }
}

fn conflict_with_block_delimiters(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    block_delimiters_style(cx) == Some("line_count_based")
        && style(cx) == "no_space"
        && node.is_multiline()
}

fn block_delimiters_style<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    cx.config
        .for_cop("Style/BlockDelimiters")
        .get("EnforcedStyle")
        .and_then(serde_yaml_ng::Value::as_str)
}

fn empty_braces(loc: &Loc) -> bool {
    loc.begin.unwrap().end_pos == loc.end.unwrap().begin_pos
}
