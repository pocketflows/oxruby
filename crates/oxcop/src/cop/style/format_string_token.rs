//! Port of `lib/rubocop/cop/style/format_string_token.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{MapKind, Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::{correct_style_detected, style, style_detected};
use crate::cop::utils::format_string::{FormatSequence, FormatString};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static FORMAT_STRING_IN_TYPICAL_CONTEXT: NodePattern = NodePattern::new(
    "{
      ^(send _ {:format :sprintf :printf} %0 ...)
      ^(send %0 :% _)
    }",
);

#[derive(Default)]
pub struct FormatStringToken;

impl Cop for FormatStringToken {
    fn name(&self) -> &'static str {
        "Style/FormatStringToken"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if format_string_token(node) || use_allowed_method(cx, node) {
            return;
        }
        let detections = collect_detections(cx, node);
        if detections.is_empty() || allowed_unannotated(cx, &detections) {
            return;
        }
        for (detected_sequence, token_range) in &detections {
            check_sequence(cx, node, detected_sequence, *token_range);
        }
    }
}

fn format_string_in_typical_context(node: Node<'_>) -> bool {
    FORMAT_STRING_IN_TYPICAL_CONTEXT.is_match(node)
}

fn format_string_token(node: Node<'_>) -> bool {
    !node.str_content().is_some_and(|v| v.contains('%'))
        || node
            .ancestors()
            .any(|a| a.is_any(&[NodeType::Xstr, NodeType::Regexp]))
}

fn use_allowed_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.ancestors()
        .find(|a| a.is(NodeType::Send))
        .and_then(|send| send.method_name())
        .is_some_and(|name| allowed_method(cx, name) || matches_allowed_pattern(cx, name))
}

fn check_sequence(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    detected_sequence: &FormatSequence,
    token_range: Range,
) {
    let style = style(cx);
    if detected_sequence.style() == style {
        correct_style_detected(cx, "EnforcedStyle");
    } else if correctable_sequence(style, detected_sequence.ty.as_deref()) {
        style_detected(cx, "EnforcedStyle", &[detected_sequence.style()]);
        register_offense(cx, node, detected_sequence, token_range);
    }
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    detected_sequence: &FormatSequence,
    token_range: Range,
) {
    let style = style(cx);
    let msg = format!(
        "Prefer {} over {}.",
        message_text(style),
        message_text(detected_sequence.style())
    );
    if format_string_context(node) {
        cx.add_offense_with_correction(token_range, &msg, |corrector| {
            if let Some(correction) = autocorrect_sequence(style, detected_sequence) {
                corrector.replace(token_range, correction);
            }
        });
    } else {
        cx.add_offense(token_range, &msg);
    }
}

fn format_string_context(node: Node<'_>) -> bool {
    format_string_in_typical_context(node)
        || node
            .ancestors()
            .filter(|a| a.is(NodeType::Dstr))
            .any(format_string_in_typical_context)
}

fn correctable_sequence(style: &str, detected_type: Option<&str>) -> bool {
    detected_type == Some("s") || style == "annotated" || style == "unannotated"
}

fn autocorrect_sequence(style: &str, detected_sequence: &FormatSequence) -> Option<String> {
    if style == "unannotated" {
        return None;
    }
    let name = detected_sequence.name.as_deref()?;
    let flags = &detected_sequence.flags;
    let width = detected_sequence.width.as_deref().unwrap_or("");
    let precision = detected_sequence.precision.as_deref().unwrap_or("");
    let ty = if detected_sequence.style() == "template" {
        "s"
    } else {
        detected_sequence.ty.as_deref().unwrap_or("")
    };
    match style {
        "annotated" => Some(format!("%<{name}>{flags}{width}{precision}{ty}")),
        "template" => Some(format!("%{flags}{width}{precision}{{{name}}}")),
        _ => None,
    }
}

fn allowed_string(cx: &Investigation<'_, '_>, node: Node<'_>, detected_style: &str) -> bool {
    (detected_style == "unannotated" || conservative(cx)) && !format_string_in_typical_context(node)
}

fn message_text(style: &str) -> &'static str {
    match style {
        "annotated" => "annotated tokens (like `%<foo>s`)",
        "template" => "template tokens (like `%{foo}`)",
        "unannotated" => "unannotated tokens (like `%s`)",
        _ => "",
    }
}

fn str_contents(node: Node<'_>) -> Range {
    let loc = node.loc();
    if loc.kind == MapKind::Heredoc {
        loc.heredoc_body.unwrap()
    } else if loc.begin.is_some() {
        node.source_range().adjust(1, -1)
    } else {
        node.source_range()
    }
}

fn tokens(cx: &Investigation<'_, '_>, node: Node<'_>) -> Vec<(FormatSequence, Range)> {
    if node.source() == "__FILE__" {
        return Vec::new();
    }
    let contents = str_contents(node);
    let format_string = FormatString::new(cx.source(contents));
    format_string
        .format_sequences()
        .iter()
        .filter(|s| !s.is_percent())
        .map(|s| {
            let token = Range::new(contents.begin_pos + s.begin_pos, contents.begin_pos + s.end_pos);
            (s.clone(), token)
        })
        .collect()
}

fn collect_detections(cx: &Investigation<'_, '_>, node: Node<'_>) -> Vec<(FormatSequence, Range)> {
    tokens(cx, node)
        .into_iter()
        .filter(|(s, _)| !allowed_string(cx, node, s.style()))
        .collect()
}

fn allowed_unannotated(cx: &Investigation<'_, '_>, detections: &[(FormatSequence, Range)]) -> bool {
    if !detections.iter().all(|(s, _)| s.style() == "unannotated") {
        return false;
    }
    let max = cx
        .cop_config_int("MaxUnannotatedPlaceholdersAllowed")
        .unwrap_or(1);
    if (detections.len() as i64) <= max {
        return true;
    }
    let style = style(cx);
    detections
        .iter()
        .any(|(s, _)| !correctable_sequence(style, s.ty.as_deref()))
}

fn conservative(cx: &Investigation<'_, '_>) -> bool {
    // `cop_config.fetch('Mode', :aggressive).to_sym`
    let mode = cx.cop_config_str("Mode").unwrap_or("aggressive");
    mode.strip_prefix(':').unwrap_or(mode) == "conservative"
}
