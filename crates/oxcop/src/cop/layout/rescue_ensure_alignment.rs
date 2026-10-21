//! Port of `lib/rubocop/cop/layout/rescue_ensure_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::end_keyword_alignment::start_line_range;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const ANCESTOR_TYPES: &[NodeType] = &[
    NodeType::Kwbegin,
    NodeType::Def,
    NodeType::Defs,
    NodeType::Class,
    NodeType::Module,
    NodeType::Sclass,
    NodeType::Block,
    NodeType::Numblock,
    NodeType::Itblock,
];
const ALTERNATIVE_ACCESS_MODIFIERS: &[&str] = &["public_class_method", "private_class_method"];

#[derive(Default)]
pub struct RescueEnsureAlignment {
    modifier_locations: Vec<Range>,
}

impl Cop for RescueEnsureAlignment {
    fn name(&self) -> &'static str {
        "Layout/RescueEnsureAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody, NodeType::Ensure]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        self.modifier_locations = cx
            .processed_source
            .tokens()
            .iter()
            .filter(|t| t.is_rescue_modifier())
            .map(|t| t.range)
            .collect();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Resbody) && self.modifier(node) {
            return;
        }
        check(cx, node);
    }
}

impl RescueEnsureAlignment {
    fn modifier(&self, node: Node<'_>) -> bool {
        node.loc()
            .keyword
            .is_some_and(|kw| self.modifier_locations.contains(&kw))
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let ps = cx.processed_source;
    let Some(alignment_node) = alignment_node(ps, node) else {
        return;
    };
    let alignment_loc = alignment_location(cx, alignment_node);
    let Some(kw_loc) = node.loc().keyword else {
        return;
    };
    if ps.column_of(alignment_loc.begin_pos) == ps.column_of(kw_loc.begin_pos)
        || same_line(ps, alignment_loc, kw_loc)
    {
        return;
    }
    let message = format!(
        "`{}` at {}, {} is not aligned with `{}` at {}, {}.",
        ps.slice(kw_loc),
        ps.line_of(kw_loc.begin_pos),
        ps.column_of(kw_loc.begin_pos),
        alignment_source(ps, alignment_node, alignment_loc),
        ps.line_of(alignment_loc.begin_pos),
        ps.column_of(alignment_loc.begin_pos)
    );
    let new_column = ps.column_of(alignment_loc.begin_pos);
    let whitespace = range_between(ps.line_start(ps.line_of(kw_loc.begin_pos)), kw_loc.begin_pos);
    let whitespace_source = ps.slice(whitespace);
    cx.add_offense_with_correction(kw_loc, &message, |corrector| {
        if !whitespace_source.trim().is_empty() {
            return;
        }
        corrector.replace(whitespace, " ".repeat(new_column));
    });
}

fn alignment_source(ps: &ProcessedSource<'_>, node: Node<'_>, starting_loc: Range) -> String {
    use NodeType::*;
    let ending_loc = match node.ty() {
        Block | Numblock | Itblock | Kwbegin => node.loc().begin,
        Def | Defs | Class | Module | Lvasgn | Ivasgn | Cvasgn | Gvasgn | Casgn => node.loc().name,
        Sclass => node.identifier().map(|i| i.source_range()),
        Masgn => node.child(0).map(|lhs| lhs.source_range()),
        _ => node
            .receiver()
            .map(|r| r.source_range())
            .or_else(|| node.child_nodes().next().and_then(|c| c.loc().name)),
    };
    let Some(ending_loc) = ending_loc else {
        return String::new();
    };
    ps.slice(range_between(starting_loc.begin_pos, ending_loc.end_pos))
        .to_owned()
}

fn alignment_node<'a>(ps: &ProcessedSource<'_>, node: Node<'a>) -> Option<Node<'a>> {
    let ancestor_node = node.ancestors().find(|a| a.is_any(ANCESTOR_TYPES))?;
    if ancestor_node.is(NodeType::Kwbegin) {
        return Some(ancestor_node);
    }
    if ancestor_node.is_any_block() && aligned_with_line_break_method(ps, ancestor_node, node) {
        return None;
    }
    if let Some(assignment_node) = assignment_node(ancestor_node)
        && same_line(ps, ancestor_node.source_range(), assignment_node.source_range())
    {
        return Some(assignment_node);
    }
    if let Some(access_modifier_node) = access_modifier_node(ancestor_node) {
        return Some(access_modifier_node);
    }
    Some(ancestor_node)
}

fn aligned_with_line_break_method(ps: &ProcessedSource<'_>, ancestor_node: Node<'_>, node: Node<'_>) -> bool {
    let Some(send_node) = ancestor_node.send_node() else {
        return false;
    };
    let send_node_loc = send_node.loc();
    let do_keyword_line = ps.line_of(ancestor_node.loc().begin.unwrap().begin_pos);
    let rescue_keyword_column = ps.column_of(node.loc().keyword.unwrap().begin_pos);
    // `send_node_loc.respond_to?(:selector) ? send_node_loc.selector : send_node_loc`
    let selector = if send_node.is_call() {
        send_node_loc.selector
    } else {
        send_node_loc.expression
    };
    if let Some(dot) = send_node_loc.dot
        && do_keyword_line == ps.line_of(dot.begin_pos)
        && rescue_keyword_column == ps.column_of(dot.begin_pos)
    {
        return true;
    }
    selector.is_some_and(|selector| {
        do_keyword_line == ps.line_of(selector.begin_pos)
            && rescue_keyword_column == ps.column_of(selector.begin_pos)
    })
}

fn assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    let is_assignment = if parent.is_call() {
        parent.is_assignment_method()
    } else {
        parent.is_assignment()
    };
    is_assignment.then_some(parent)
}

fn access_modifier_node(node: Node<'_>) -> Option<Node<'_>> {
    if !node.is_any_def() {
        return None;
    }
    let parent = node.parent()?;
    access_modifier(parent).then_some(parent)
}

fn access_modifier(node: Node<'_>) -> bool {
    if node.is_send() && node.is_access_modifier() {
        return true;
    }
    node.method_name()
        .is_some_and(|m| ALTERNATIVE_ACCESS_MODIFIERS.contains(&m))
}

fn alignment_location(cx: &Investigation<'_, '_>, alignment_node: Node<'_>) -> Range {
    let ps = cx.processed_source;
    if begin_end_alignment_style(cx) == Some("start_of_line") {
        start_line_range(ps, alignment_node)
    } else if alignment_node.is_any_block() {
        let send_node = alignment_node.send_node().unwrap();
        let range = ps.line_range(send_node.last_line());
        let source = ps.slice(range);
        let offset = source
            .find(|c: char| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
            .unwrap_or(0);
        range.with_begin(range.begin_pos + offset)
    } else {
        alignment_node.source_range()
    }
}

fn begin_end_alignment_style<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    // `conf['Enabled']` is computed by `Config#for_cop`.
    if !cx.config.cop_enabled_in_config("Layout/BeginEndAlignment") {
        return None;
    }
    cx.config
        .for_cop("Layout/BeginEndAlignment")
        .get("EnforcedStyleAlignWith")
        .and_then(serde_yaml_ng::Value::as_str)
}
