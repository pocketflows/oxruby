//! Port of `lib/rubocop/cop/layout/empty_lines_around_access_modifier.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::{range_by_whole_lines, source_range};
use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG_AFTER: &str = "Keep a blank line after `%<modifier>s`.";
const MSG_BEFORE_AND_AFTER: &str = "Keep a blank line before and after `%<modifier>s`.";
const MSG_BEFORE_FOR_ONLY_BEFORE: &str = "Keep a blank line before `%<modifier>s`.";
const MSG_AFTER_FOR_ONLY_BEFORE: &str = "Remove a blank line after `%<modifier>s`.";

#[derive(Default)]
pub struct EmptyLinesAroundAccessModifier {
    block_line: Option<usize>,
    class_or_module_def_first_line: Option<usize>,
    class_or_module_def_last_line: Option<usize>,
}

impl Cop for EmptyLinesAroundAccessModifier {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundAccessModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Class, Module, Sclass, Block, Numblock, Itblock, Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["public", "protected", "private", "module_function"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            Class => {
                self.class_or_module_def_first_line = Some(match node.parent_class() {
                    Some(parent_class) => parent_class.first_line(),
                    None => node.first_line(),
                });
                self.class_or_module_def_last_line = Some(node.last_line());
            }
            Module => {
                self.class_or_module_def_first_line = Some(node.first_line());
                self.class_or_module_def_last_line = Some(node.last_line());
            }
            Sclass => {
                self.class_or_module_def_first_line = Some(node.identifier().unwrap().first_line());
                self.class_or_module_def_last_line = Some(node.last_line());
            }
            Block | Numblock | Itblock => self.block_line = Some(node.first_line()),
            Send => self.on_send(cx, node),
            _ => {}
        }
    }
}

fn blank(line: &str) -> bool {
    line.chars().all(char::is_whitespace)
}

impl EmptyLinesAroundAccessModifier {
    fn on_send<'a>(&self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_bare_access_modifier() && !node.is_block_literal()) {
            return;
        }
        if node
            .right_sibling()
            .is_some_and(|s| s.first_line() == node.first_line())
        {
            return;
        }
        let style = style(cx);
        let ps = cx.processed_source;
        if self.expected_empty_lines(ps, style, node) {
            return;
        }
        let message = self.message(ps, style, node);
        let no_empty_lines_around_block_body = cx
            .config
            .for_enabled_cop("Layout/EmptyLinesAroundBlockBody")
            .get("EnforcedStyle")
            .and_then(Value::as_str)
            == Some("no_empty_lines");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let line = range_by_whole_lines(ps, node.source_range(), false);
            if self.should_insert_line_before(ps, node, no_empty_lines_around_block_body) {
                corrector.insert_before(line, "\n");
            }
            // correct_next_line_if_denied_style
            if !should_insert_line_after(node, no_empty_lines_around_block_body) {
                return;
            }
            match style {
                "around" if !self.next_line_empty(ps, node.last_line()) => {
                    corrector.insert_after(line, "\n");
                }
                "only_before" if self.next_line_empty_and_exists(ps, node.last_line()) => {
                    corrector.remove(source_range(ps, node.last_line() + 1, 0, 1));
                }
                _ => {}
            }
        });
    }

    fn expected_empty_lines(&self, ps: &ProcessedSource<'_>, style: &str, node: Node<'_>) -> bool {
        match style {
            "around" => self.empty_lines_around(ps, node),
            "only_before" => self.allowed_only_before_style(ps, node),
            _ => false,
        }
    }

    fn allowed_only_before_style(&self, ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
        if node.is_special_modifier() {
            if ps.lines().get(node.last_line()) == Some(&"end") {
                return true;
            }
            if self.next_line_empty_and_exists(ps, node.last_line()) {
                return false;
            }
        }
        self.previous_line_empty(ps, node.first_line())
    }

    fn previous_line_empty(&self, ps: &ProcessedSource<'_>, send_line: usize) -> bool {
        let lines = ps.lines();
        // `processed_source[0..(send_line - 2)]`: `0..-1` (all lines) on line 1.
        let previous: &[&str] = if send_line >= 2 {
            &lines[..(send_line - 1).min(lines.len())]
        } else {
            lines
        };
        let Some(previous_line) = previous.iter().rev().find(|l| !comment_line(l)) else {
            return true;
        };
        self.block_start(send_line) || self.class_def(send_line) || blank(previous_line)
    }

    fn next_line_empty(&self, ps: &ProcessedSource<'_>, last_send_line: usize) -> bool {
        self.body_end(last_send_line) || ps.lines().get(last_send_line).is_none_or(|l| blank(l))
    }

    fn next_line_empty_and_exists(&self, ps: &ProcessedSource<'_>, last_send_line: usize) -> bool {
        self.next_line_empty(ps, last_send_line) && last_send_line + 1 != ps.lines().len()
    }

    fn empty_lines_around(&self, ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
        self.previous_line_empty(ps, node.first_line()) && self.next_line_empty(ps, node.last_line())
    }

    fn class_def(&self, line: usize) -> bool {
        self.class_or_module_def_first_line
            .is_some_and(|first| line == first + 1)
    }

    fn block_start(&self, line: usize) -> bool {
        self.block_line.is_some_and(|block_line| line == block_line + 1)
    }

    fn body_end(&self, line: usize) -> bool {
        self.class_or_module_def_last_line
            .is_some_and(|last| line + 1 == last)
    }

    fn message(&self, ps: &ProcessedSource<'_>, style: &str, node: Node<'_>) -> String {
        let modifier = ps.slice(node.loc().selector.unwrap());
        let template = match style {
            "around" => {
                let send_line = node.first_line();
                if self.block_start(send_line) || self.class_def(send_line) {
                    MSG_AFTER
                } else {
                    MSG_BEFORE_AND_AFTER
                }
            }
            "only_before" => {
                if self.next_line_empty(ps, node.last_line()) {
                    MSG_AFTER_FOR_ONLY_BEFORE
                } else {
                    MSG_BEFORE_FOR_ONLY_BEFORE
                }
            }
            _ => return String::new(),
        };
        template.replace("%<modifier>s", modifier)
    }

    fn should_insert_line_before(
        &self,
        ps: &ProcessedSource<'_>,
        node: Node<'_>,
        no_empty_lines_around_block_body: bool,
    ) -> bool {
        if self.previous_line_empty(ps, node.first_line()) {
            return false;
        }
        if !(inside_block(node) && no_empty_lines_around_block_body) {
            return true;
        }
        let parent = node.parent().unwrap();
        if !parent.is(NodeType::Begin) {
            return true;
        }
        parent.child(0) != Some(node)
    }
}

fn should_insert_line_after(node: Node<'_>, no_empty_lines_around_block_body: bool) -> bool {
    if !(inside_block(node) && no_empty_lines_around_block_body) {
        return true;
    }
    node.parent().unwrap().child_nodes().last() != Some(node)
}

fn inside_block(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::Block)
        || (parent.is(NodeType::Begin) && parent.parent().is_some_and(|p| p.is(NodeType::Block)))
}
