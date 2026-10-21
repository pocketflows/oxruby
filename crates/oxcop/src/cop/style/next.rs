//! Port of `lib/rubocop/cop/style/next.rb`.

use std::collections::HashMap;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::min_body_length::min_body_length;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

const MSG: &str = "Use `next` to skip iteration.";

#[derive(Default)]
pub struct Next {
    /// `@reindented_lines`: how much each line was already reindented.
    reindented_lines: HashMap<usize, isize>,
}

impl Cop for Next {
    fn name(&self) -> &'static str {
        "Style/Next"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/SafeNavigation"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::While,
            NodeType::Until,
            NodeType::For,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.reindented_lines.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block()
            && !node
                .send_node()
                .is_some_and(|s| s.is_call() && s.is_enumerator_method())
        {
            return;
        }
        self.check(cx, node);
    }
}

impl Next {
    fn check<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body() else {
            return;
        };
        if !ends_with_condition(cx, body) {
            return;
        }
        let offending_node = offense_node(body);
        if cx.cop_config_bool("AllowConsecutiveConditionals", false)
            && consecutive_conditionals(offending_node)
        {
            return;
        }
        let location = offending_node
            .source_range()
            .begin()
            .join(offending_node.condition().unwrap().source_range());
        let ps = cx.processed_source;
        let reindented_lines = &mut self.reindented_lines;
        cx.add_offense_with_correction(location, MSG, |corrector| {
            if offending_node.is_modifier_form() {
                autocorrect_modifier(corrector, offending_node);
            } else {
                autocorrect_block(ps, corrector, offending_node, reindented_lines);
            }
        });
    }
}

fn ends_with_condition(cx: &Investigation<'_, '_>, body: Node<'_>) -> bool {
    if simple_if_without_break(cx, Some(body)) {
        return true;
    }
    body.is(NodeType::Begin) && simple_if_without_break(cx, body.child_nodes().last())
}

fn simple_if_without_break(cx: &Investigation<'_, '_>, node: Option<Node<'_>>) -> bool {
    let Some(node) = node.filter(|n| n.is(NodeType::If) && !n.is_ternary() && !n.has_else())
    else {
        return false;
    };
    if node
        .child_nodes()
        .any(|c| c.is(NodeType::If) && c.has_else())
    {
        return false;
    }
    let allowed_modifier_if = if node.is_modifier_form() {
        style(cx) == "skip_modifier_ifs"
    } else {
        !min_body_length(cx, node)
    };
    if allowed_modifier_if {
        return false;
    }
    !node
        .if_branch()
        .is_some_and(|b| matches!(b.ty(), NodeType::Break | NodeType::Return))
}

/// `*_, condition = *body; condition&.if_type? ? condition : body`.
fn offense_node(body: Node<'_>) -> Node<'_> {
    match body.children().last() {
        Some(Child::Node(id)) => {
            let condition = body.ast().node(*id);
            if condition.is(NodeType::If) {
                condition
            } else {
                body
            }
        }
        _ => body,
    }
}

fn autocorrect_modifier(corrector: &mut Corrector, node: Node<'_>) {
    let body = node.if_branch().or(node.else_branch()).unwrap();
    let replacement = format!(
        "next {} {}\n{}{}",
        node.inverse_keyword(),
        node.condition().unwrap().source(),
        " ".repeat(node.column()),
        body.source()
    );
    corrector.replace(node.source_range(), replacement);
}

fn autocorrect_block(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    reindented_lines: &mut HashMap<usize, isize>,
) {
    let condition = node.condition().unwrap();
    let next_code = format!("next {} {}", node.inverse_keyword(), condition.source());
    corrector.insert_before(node.source_range(), next_code);
    corrector.remove(cond_range(node, condition));
    corrector.remove(end_range(ps, node));
    let lines = reindentable_lines(ps, node);
    if lines.is_empty() {
        return;
    }
    reindent(ps, &lines, condition, corrector, reindented_lines);
}

fn cond_range(node: Node<'_>, cond: Node<'_>) -> Range {
    let end_pos = match node.loc().begin {
        Some(begin) => begin.end_pos,
        None => cond.source_range().end_pos,
    };
    range_between(node.source_range().begin_pos, end_pos)
}

fn end_range(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let end = node.loc().end.unwrap();
    let mut begin_pos = ps.line_start(ps.line_of(end.begin_pos));
    if regex!(r"\A\s*$")
        .find(&ps.buffer_source()[end.end_pos..])
        .is_some()
    {
        begin_pos -= 1;
    }
    range_between(begin_pos, end.end_pos)
}

fn reindentable_lines(ps: &ProcessedSource<'_>, node: Node<'_>) -> Vec<usize> {
    let heredoc_lines: Vec<usize> = node
        .each_node()
        .filter(|n| n.is(NodeType::Dstr) && n.is_heredoc())
        .filter_map(|n| n.loc().heredoc_body)
        .flat_map(|b| ps.line_of(b.begin_pos)..ps.line_of(b.end_pos))
        .collect();
    let end_line = ps.line_of(node.loc().end.unwrap().begin_pos);
    ((node.first_line() + 1)..end_line)
        .filter(|l| !heredoc_lines.contains(l))
        .filter(|&l| regex!(r"\A\s*\z").find(ps.source_line(l)).is_none())
        .collect()
}

fn first_non_space(line: &str) -> Option<usize> {
    regex!(r"\S").find(line).map(|(b, _)| line[..b].chars().count())
}

fn reindent(
    ps: &ProcessedSource<'_>,
    lines: &[usize],
    node: Node<'_>,
    corrector: &mut Corrector,
    reindented_lines: &mut HashMap<usize, isize>,
) {
    let target_indent = first_non_space(ps.source_line(node.first_line())).unwrap_or(0) as isize;
    let actual_indent = lines
        .iter()
        .filter_map(|&l| first_non_space(ps.source_line(l)))
        .min()
        .unwrap_or(0) as isize;
    let delta = actual_indent - target_indent;
    for &lineno in lines {
        let adjustment = delta + reindented_lines.get(&lineno).copied().unwrap_or(0);
        reindented_lines.insert(lineno, adjustment);
        if adjustment > 0 {
            corrector.remove_leading(ps.line_range(lineno), adjustment as usize);
        }
    }
}

fn consecutive_conditionals(if_node: Node<'_>) -> bool {
    if_node.parent().is_some_and(|p| p.is(NodeType::Begin))
        && if_node.left_sibling().is_some_and(|s| s.is(NodeType::If))
}
