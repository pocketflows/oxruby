//! Port of `lib/rubocop/cop/layout/indentation_width.rb`.
//!
//! Base locations (`base_loc`) are ranges: a `Map` stands for its
//! expression, as `column`, `line` and `source_line` treat it.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{configured_indentation_width, within};
use crate::cop::mixin::allowed_pattern::allowed_line;
use crate::cop::mixin::check_assignment;
use crate::cop::mixin::end_keyword_alignment::{style, variable_alignment};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{self, char_offset, range_between, source_range};
use crate::cop::mixin::util::{begins_its_line, first_part_of_call_chain, parentheses};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static ACCESS_MODIFIER: NodePattern = NodePattern::new("[(send ...) access_modifier?]");

#[derive(Default)]
pub struct IndentationWidth {
    ignored: IgnoredNodes,
    /// `@offense_ranges`: kept for the cop's lifetime, as in Ruby.
    offense_ranges: Vec<Range>,
}

impl Cop for IndentationWidth {
    fn name(&self) -> &'static str {
        "Layout/IndentationWidth"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            Rescue, Resbody, For, Ensure, Kwbegin, Begin, Block, Numblock, Itblock, Class, Sclass,
            Module, Send, Csend, Def, Defs, While, Until, Case, CaseMatch, If, Lvasgn, Ivasgn,
            Cvasgn, Gvasgn, Casgn, Masgn, OpAsgn, OrAsgn, AndAsgn,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        let ps = cx.processed_source;
        match node.ty() {
            Rescue => self.check_indentation(cx, node.loc().else_, node.else_branch(), "normal"),
            Resbody | For => self.check_indentation(cx, node.loc().keyword, node.body(), "normal"),
            Ensure => self.check_indentation(cx, node.loc().keyword, node.ensure_branch(), "normal"),
            Kwbegin => {
                let Some(end) = node.loc().end.filter(|e| begins_its_line(ps, *e)) else {
                    return;
                };
                self.check_indentation(cx, Some(end), node.child(0), "normal");
            }
            Begin => {
                if !(parentheses(node) && begins_its_line(ps, node.loc().end.unwrap())) {
                    return;
                }
                let begin = node.loc().begin.unwrap();
                self.check_indentation(cx, Some(opening_line_start(cx, begin)), node.child(0), "normal");
            }
            Block | Numblock | Itblock => self.on_block(cx, node),
            Class | Sclass | Module => {
                let base = node.loc().keyword.unwrap();
                if node
                    .body()
                    .is_some_and(|body| body.first_line() == ps.line_of(base.begin_pos))
                {
                    return;
                }
                self.check_members(cx, base, node.body());
            }
            Send | Csend => self.on_send(cx, node),
            Def | Defs => {
                if self.ignored.contains(node) {
                    return;
                }
                self.check_indentation(cx, node.loc().keyword, node.body(), "normal");
            }
            While | Until => self.on_while(cx, node, node),
            Case => {
                let whens = node.when_branches();
                for when_node in &whens {
                    self.check_indentation(cx, when_node.loc().keyword, when_node.body(), "normal");
                }
                if let Some(last) = whens.last() {
                    self.check_indentation(cx, last.loc().keyword, node.else_branch(), "normal");
                }
            }
            CaseMatch => {
                let branches: Vec<Node<'a>> = node
                    .child_nodes()
                    .filter(|c| c.is(NodeType::InPattern))
                    .collect();
                for in_pattern_node in &branches {
                    self.check_indentation(
                        cx,
                        in_pattern_node.loc().keyword,
                        in_pattern_node.body(),
                        "normal",
                    );
                }
                let else_branch = node.else_branch().filter(|e| !e.is(NodeType::EmptyElse));
                if let Some(last) = branches.last() {
                    self.check_indentation(cx, last.loc().keyword, else_branch, "normal");
                }
            }
            If => self.on_if(cx, node, node),
            _ => {
                if let Some((node, rhs)) = check_assignment::assignment(node) {
                    self.check_assignment(cx, node, rhs);
                }
            }
        }
    }
}

impl IndentationWidth {
    fn on_block<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let end_loc = node.loc().end.unwrap();
        if !begins_its_line(cx.processed_source, end_loc) {
            return;
        }
        let base_loc = block_body_indentation_base(cx, node, end_loc);
        self.check_indentation(cx, Some(base_loc), node.body(), "normal");
        if !indented_internal_methods_style(cx) || !contains_access_modifier(node.body()) {
            return;
        }
        self.check_members(cx, end_loc, node.body());
    }

    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // `super`: CheckAssignment#on_send (also for `csend`, which is an
        // alias of `on_send` here).
        if let Some(rhs) = check_assignment::extract_rhs(node) {
            self.check_assignment(cx, node, Some(rhs));
        }
        if !adjacent_def_modifier(node) {
            return;
        }
        let def_end_style = cx
            .config
            .for_cop("Layout/DefEndAlignment")
            .get("EnforcedStyleAlignWith")
            .and_then(Value::as_str)
            .unwrap_or("start_of_line");
        let first_argument = node.first_argument().unwrap();
        let base = if def_end_style == "def" {
            first_argument
        } else {
            leftmost_modifier_of(node)
        };
        self.check_indentation(cx, Some(base.source_range()), first_argument.body(), "normal");
        self.ignored.ignore(first_argument);
    }

    fn on_while<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, base: Node<'a>) {
        if self.ignored.contains(node) || !node.is_single_line_condition() {
            return;
        }
        self.check_indentation(cx, Some(base.source_range()), node.body(), "normal");
    }

    fn on_if<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, base: Node<'a>) {
        if self.ignored.contains(node) || node.is_ternary() || node.is_modifier_form() {
            return;
        }
        self.check_if(cx, node, node.body(), node.else_branch(), base.source_range());
    }

    fn check_members<'a>(&mut self, cx: &mut Investigation<'a, '_>, base: Range, member: Option<Node<'a>>) {
        let checked = select_check_member(cx, member);
        self.check_indentation(cx, Some(base), checked, "normal");
        let Some(member) = member.filter(|m| m.is(NodeType::Begin)) else {
            return;
        };
        if indented_internal_methods_style(cx) {
            let style = indentation_consistency_style(cx);
            for (member, previous_modifier) in each_member(member) {
                self.check_indentation(cx, Some(previous_modifier), Some(member), style);
            }
        } else {
            for child in member.child_nodes() {
                if child.is_send() && child.is_access_modifier() {
                    continue;
                }
                self.check_indentation(cx, Some(base), Some(child), "normal");
            }
        }
    }

    fn check_assignment<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, rhs: Option<Node<'a>>) {
        let Some(rhs) = rhs.and_then(first_part_of_call_chain) else {
            return;
        };
        let end_style = cx
            .config
            .for_cop("Layout/EndAlignment")
            .get("EnforcedStyleAlignWith")
            .and_then(Value::as_str)
            .unwrap_or("keyword");
        let base = if variable_alignment(cx.processed_source, node.source_range(), rhs, end_style) {
            node
        } else {
            rhs
        };
        match rhs.ty() {
            NodeType::If => self.on_if(cx, rhs, base),
            NodeType::While | NodeType::Until => self.on_while(cx, rhs, base),
            _ => return,
        }
        self.ignored.ignore(rhs);
    }

    fn check_if<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        body: Option<Node<'a>>,
        else_clause: Option<Node<'a>>,
        base_loc: Range,
    ) {
        if node.is_ternary() {
            return;
        }
        self.check_indentation(cx, Some(base_loc), body, "normal");
        let Some(else_clause) = else_clause else {
            return;
        };
        if else_clause.is(NodeType::If) && else_clause.is_elsif() {
            return;
        }
        self.check_indentation(cx, node.loc().else_, Some(else_clause), "normal");
    }

    fn check_indentation<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        base_loc: Option<Range>,
        body_node: Option<Node<'a>>,
        style: &str,
    ) {
        if !indentation_to_check(cx, base_loc, body_node) {
            return;
        }
        let (Some(base_loc), Some(body_node)) = (base_loc, body_node) else {
            return;
        };
        let indentation = column_offset_between(cx, body_node.source_range(), base_loc);
        let column_delta = configured_indentation_width(cx) as isize - indentation;
        if column_delta == 0 {
            return;
        }
        self.offense(cx, body_node, indentation, style, column_delta);
    }

    fn offense<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        body_node: Node<'a>,
        indentation: isize,
        style: &str,
        column_delta: isize,
    ) {
        let body_node = if body_node.is(NodeType::Begin) && !parentheses(body_node) {
            body_node.child(0).unwrap_or(body_node)
        } else {
            body_node
        };
        let node = if cx.autocorrect() && self.other_offense_in_same_range(body_node) {
            None
        } else {
            Some(body_node)
        };
        let name = if style == "normal" {
            String::new()
        } else {
            format!(" {style}")
        };
        let message = message(cx, configured_indentation_width(cx) as isize, indentation, &name);
        let range = offending_range(cx, body_node, indentation);
        cx.add_offense_cx(range, &message, |corrector, cx| {
            let Some(node) = node else {
                return;
            };
            alignment_corrector::correct(
                corrector,
                cx.processed_source,
                cx.config,
                Some(Target::Node(node)),
                column_delta,
                true,
            );
        });
    }

    fn other_offense_in_same_range(&mut self, node: Node<'_>) -> bool {
        let expr = node.source_range();
        if self.offense_ranges.iter().any(|r| within(expr, *r)) {
            return true;
        }
        self.offense_ranges.push(expr);
        false
    }
}

fn adjacent_def_modifier(node: Node<'_>) -> bool {
    node.is(NodeType::Send)
        && node.receiver().is_none()
        && node.children().len() == 3
        && node.child(2).is_some_and(|arg| arg.is_any_def())
}

fn opening_line_start(cx: &Investigation<'_, '_>, begin_loc: Range) -> Range {
    let ps = cx.processed_source;
    let line = ps.line_of(begin_loc.begin_pos);
    let column = ps
        .source_line(line)
        .chars()
        .position(|c| !c.is_whitespace())
        .unwrap_or(0);
    source_range(ps, line, column, 1)
}

fn select_check_member<'a>(cx: &Investigation<'_, '_>, member: Option<Node<'a>>) -> Option<Node<'a>> {
    let member = member?;
    match member.child(0) {
        Some(first) if ACCESS_MODIFIER.is_match(first) => {
            if access_modifier_indentation_style(cx) == Some("outdent") {
                None
            } else {
                Some(first)
            }
        }
        _ => Some(member),
    }
}

fn each_member(members: Node<'_>) -> Vec<(Node<'_>, Range)> {
    let mut result = Vec::new();
    let mut previous_modifier: Option<Node<'_>> = None;
    for member in members.child_nodes() {
        if member.is_send() && member.is_special_modifier() {
            previous_modifier = Some(member);
        } else if let Some(modifier) = previous_modifier.take() {
            result.push((member, modifier.source_range()));
        }
    }
    result
}

fn indented_internal_methods_style(cx: &Investigation<'_, '_>) -> bool {
    indentation_consistency_style(cx) == "indented_internal_methods"
}

fn access_modifier_indentation_style<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    cx.config
        .for_cop("Layout/AccessModifierIndentation")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
}

fn indentation_consistency_style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    cx.config
        .for_cop("Layout/IndentationConsistency")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn message(cx: &Investigation<'_, '_>, configured_indentation_width: isize, indentation: isize, name: &str) -> String {
    if using_tabs(cx) {
        let actual_tabs = indentation.div_euclid(configured_indentation_width.max(1));
        format!("Use 1 (not {actual_tabs}) tabs for{name} indentation.")
    } else {
        format!("Use {configured_indentation_width} (not {indentation}) spaces for{name} indentation.")
    }
}

fn indentation_to_check(cx: &Investigation<'_, '_>, base_loc: Option<Range>, body_node: Option<Node<'_>>) -> bool {
    if skip_check(cx, base_loc, body_node) {
        return false;
    }
    let body_node = body_node.unwrap();
    if body_node.is(NodeType::Rescue) {
        body_node.body().is_some()
    } else if body_node.is(NodeType::Ensure) {
        match body_node.child(0) {
            Some(block_body) if block_body.is(NodeType::Rescue) => block_body.body().is_some(),
            block_body => block_body.is_some(),
        }
    } else {
        true
    }
}

fn skip_check(cx: &Investigation<'_, '_>, base_loc: Option<Range>, body_node: Option<Node<'_>>) -> bool {
    let ps = cx.processed_source;
    if let Some(base_loc) = base_loc
        && allowed_line(cx, ps.source_line(ps.line_of(base_loc.begin_pos)))
    {
        return true;
    }
    let Some(body_node) = body_node else {
        return true;
    };
    let Some(base_loc) = base_loc else {
        // `same_line?(body_node, nil)` is false; Ruby goes on with a nil
        // base and fails in `column_offset_between`.
        return true;
    };
    if body_node.first_line() == ps.line_of(base_loc.begin_pos) {
        return true;
    }
    if starts_with_access_modifier(body_node) {
        return true;
    }
    let first_char_pos_on_line = ps
        .source_line(body_node.first_line())
        .chars()
        .position(|c| !c.is_whitespace());
    Some(body_node.column()) != first_char_pos_on_line
}

fn offending_range(cx: &Investigation<'_, '_>, body_node: Node<'_>, indentation: isize) -> Range {
    let ps = cx.processed_source;
    let expr = body_node.source_range();
    let begin_pos = expr.begin_pos;
    let line_start = ps.line_start(ps.line_of(begin_pos));
    let ind = if using_tabs(cx) {
        // `begin_pos - line_indentation(expr).length`
        line_start
    } else if indentation >= 0 {
        let chars_before = ps.buffer_source()[line_start..begin_pos].chars().count();
        let back = (indentation as usize).min(chars_before);
        char_offset(ps.buffer_source(), line_start, chars_before - back)
    } else {
        char_offset(ps.buffer_source(), begin_pos, indentation.unsigned_abs())
    };
    if ind <= begin_pos {
        range_between(ind, begin_pos)
    } else {
        range_between(begin_pos, ind)
    }
}

fn starts_with_access_modifier(body_node: Node<'_>) -> bool {
    if !body_node.is(NodeType::Begin) {
        return false;
    }
    body_node
        .child(0)
        .is_some_and(|first| first.is_send() && first.is_bare_access_modifier())
}

fn contains_access_modifier(body_node: Option<Node<'_>>) -> bool {
    body_node.filter(|b| b.is(NodeType::Begin)).is_some_and(|b| {
        b.child_nodes()
            .any(|child| child.is_send() && child.is_bare_access_modifier())
    })
}

fn using_tabs(cx: &Investigation<'_, '_>) -> bool {
    cx.config
        .for_cop("Layout/IndentationStyle")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .unwrap_or("spaces")
        == "tabs"
}

/// `column_offset_between(base_range, range)`, which counts tabs as
/// `configured_indentation_width` columns when indenting with tabs.
fn column_offset_between(cx: &Investigation<'_, '_>, base_range: Range, range: Range) -> isize {
    let ps = cx.processed_source;
    if !using_tabs(cx) {
        return range_help::column_offset_between(ps, base_range, range);
    }
    let base_uses_tabs = line_indentation(cx, base_range).contains('\t');
    let range_uses_tabs = line_indentation(cx, range).contains('\t');
    if !(base_uses_tabs || range_uses_tabs) {
        return range_help::column_offset_between(ps, base_range, range);
    }
    visual_column(cx, base_range) - visual_column(cx, range)
}

fn line_indentation<'s>(cx: &Investigation<'_, 's>, range: Range) -> &'s str {
    let ps = cx.processed_source;
    let line_start = ps.line_start(ps.line_of(range.begin_pos));
    &ps.buffer_source()[line_start..range.begin_pos]
}

fn visual_column(cx: &Investigation<'_, '_>, range: Range) -> isize {
    let indentation = line_indentation(cx, range);
    let tab_count = indentation.matches('\t').count();
    let space_count = indentation.matches(' ').count();
    (tab_count * configured_indentation_width(cx) + space_count) as isize
}

fn leftmost_modifier_of(node: Node<'_>) -> Node<'_> {
    match node.parent() {
        Some(parent) if parent.is_send() => leftmost_modifier_of(parent),
        _ => node,
    }
}

fn block_body_indentation_base(cx: &Investigation<'_, '_>, node: Node<'_>, end_loc: Range) -> Range {
    if style(cx) != "relative_to_receiver" {
        return end_loc;
    }
    let ps = cx.processed_source;
    let Some(send_node) = node.send_node() else {
        return end_loc;
    };
    let receiver_last_line = send_node.receiver().map(|r| r.last_line());
    if let Some(dot) = send_node.loc().dot
        && receiver_last_line.is_some_and(|l| l < ps.line_of(dot.begin_pos))
    {
        return dot;
    }
    if let (Some(_), Some(selector)) = (send_node.loc().dot, send_node.loc().selector)
        && receiver_last_line.is_some_and(|l| l < ps.line_of(selector.begin_pos))
    {
        return selector;
    }
    end_loc
}
