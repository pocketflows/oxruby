//! Port of `lib/rubocop/cop/layout/line_length.rb`.

use std::collections::HashMap;
use std::ops::Range as CharRange;

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::check_line_breakable::extract_breakable_node;
use crate::cop::mixin::endless_method_rewriter;
use crate::cop::mixin::line_length_help::{
    ExcessiveType, allow_cop_directives, allow_qualified_name, allow_rbs_inline_annotation,
    allow_uri, allowed_position, directive_on_source_line,
    find_excessive_range, indentation_difference, line_length, line_length_without_directive,
    rbs_inline_annotation_on_source_line,
};
use crate::cop::mixin::range_help::{char_offset, column_offset_between, range_between, source_range};
use crate::cop::mixin::util::{indent, same_line};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

#[derive(Default)]
pub struct LineLength {
    breakable_range_by_line_index: HashMap<usize, Range>,
    breakable_string_delimiters: HashMap<usize, String>,
    endless_methods_by_line: HashMap<usize, NodeId>,
    heredocs: Option<Vec<(CharRange<usize>, String)>>,
}

impl Cop for LineLength {
    fn name(&self) -> &'static str {
        "Layout/LineLength"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            Block, Numblock, Itblock, Str, Dstr, Array, Hash, Send, Csend, Def, Defs,
        ]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        self.breakable_range_by_line_index.clear();
        self.breakable_string_delimiters.clear();
        self.endless_methods_by_line.clear();
        self.heredocs = None;
        if !cx.processed_source.raw_source().contains(';') {
            return;
        }
        self.check_for_breakable_semicolons(cx.processed_source);
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            Block | Numblock | Itblock => self.check_for_breakable_block(cx, node),
            Str => self.check_for_breakable_str(cx, node),
            Dstr => self.check_for_breakable_dstr(cx, node),
            Def | Defs if node.is_endless() => {
                self.endless_methods_by_line
                    .insert(node.first_line() - 1, node.id());
            }
            _ => self.check_for_breakable_node(cx, node),
        }
    }

    fn on_investigation_end(&mut self, cx: &mut Investigation<'_, '_>) {
        let lines = cx.processed_source.lines();
        for (line_index, line) in lines.iter().enumerate() {
            self.check_line(cx, line, line_index);
        }
    }
}

fn max(cx: &Investigation<'_, '_>) -> Option<usize> {
    cx.cop_config_int("Max").map(|m| m.max(0) as usize)
}

fn max_or_inf(cx: &Investigation<'_, '_>) -> usize {
    max(cx).unwrap_or(usize::MAX)
}

fn message(length: usize, max: usize) -> String {
    format!("Line is too long. [{length}/{max}]")
}

/// `range.last_column`.
fn last_column(ps: &ProcessedSource<'_>, range: Range) -> usize {
    ps.column_of(range.end_pos)
}

impl LineLength {
    fn check_for_breakable_node(&mut self, cx: &Investigation<'_, '_>, node: Node<'_>) {
        let Some(breakable_node) = extract_breakable_node(cx.processed_source, node, max(cx))
        else {
            return;
        };
        let line_index = breakable_node.first_line() - 1;
        self.breakable_range_by_line_index
            .entry(line_index)
            .or_insert_with(|| breakable_node.source_range());
    }

    fn check_for_breakable_semicolons(&mut self, ps: &ProcessedSource<'_>) {
        for token in ps.tokens().iter().filter(|t| t.ty == "tSEMI").rev() {
            if let Some(range) = breakable_range_after_semicolon(ps, token.range) {
                self.breakable_range_by_line_index
                    .insert(ps.line_of(range.begin_pos) - 1, range);
            }
        }
    }

    fn check_for_breakable_block(&mut self, cx: &Investigation<'_, '_>, block_node: Node<'_>) {
        if !block_node.is_single_line() {
            return;
        }
        if receiver_contains_heredoc(block_node) {
            return;
        }
        let line_index = block_node.first_line() - 1;
        let range = breakable_block_range(block_node);
        let pos = range.begin_pos + 1;
        let _ = cx;
        self.breakable_range_by_line_index
            .insert(line_index, range_between(pos, pos + 1));
    }

    fn check_for_breakable_str(&mut self, cx: &Investigation<'_, '_>, node: Node<'_>) {
        let line_index = node.first_line() - 1;
        if self.breakable_range_by_line_index.contains_key(&line_index) {
            return;
        }
        if !breakable_string(cx, node) {
            return;
        }
        let Some(delimiter) = string_delimiter(node) else {
            return;
        };
        let Some(pos) = breakable_string_position(cx, node) else {
            return;
        };
        self.breakable_range_by_line_index
            .insert(line_index, range_between(pos, pos + 1));
        self.breakable_string_delimiters
            .insert(line_index, delimiter.to_owned());
    }

    fn check_for_breakable_dstr(&mut self, cx: &Investigation<'_, '_>, node: Node<'_>) {
        let line_index = node.first_line() - 1;
        if self.breakable_range_by_line_index.contains_key(&line_index) {
            return;
        }
        if !breakable_dstr(cx, node) {
            return;
        }
        let Some(delimiter) = string_delimiter(node) else {
            return;
        };
        for begin_node in node.child_nodes().filter(|c| c.is(NodeType::Begin)) {
            if let Some(pos) = breakable_dstr_begin_position(cx, begin_node) {
                self.breakable_range_by_line_index
                    .insert(line_index, range_between(pos, pos + 1));
                self.breakable_string_delimiters
                    .insert(line_index, delimiter.to_owned());
                break;
            }
        }
    }

    fn heredocs(&mut self, ps: &ProcessedSource<'_>) -> &[(CharRange<usize>, String)] {
        self.heredocs.get_or_insert_with(|| extract_heredocs(ps))
    }

    fn check_line(&mut self, cx: &mut Investigation<'_, '_>, line: &str, line_index: usize) {
        let max = max_or_inf(cx);
        if line_length(cx, line) <= max {
            return;
        }
        if self.allowed_line(cx, line, line_index) {
            return;
        }
        if self.endless_methods_by_line.contains_key(&line_index) {
            self.handle_endless_method_line(cx, line, line_index);
            return;
        }
        if allow_rbs_inline_annotation(cx) && rbs_inline_annotation_on_source_line(cx, line_index)
        {
            return;
        }
        if allow_cop_directives(cx) && directive_on_source_line(cx, line_index) {
            self.check_directive_line(cx, line, line_index);
            return;
        }
        if allow_uri(cx) || allow_qualified_name(cx) {
            self.check_line_for_exemptions(cx, line, line_index);
            return;
        }
        let loc = excess_range(cx, None, line, line_index);
        let length = line_length(cx, line);
        self.register_offense(cx, loc, line, line_index, length);
    }

    fn allowed_line(&mut self, cx: &Investigation<'_, '_>, line: &str, line_index: usize) -> bool {
        matches_allowed_pattern(cx, line)
            || (line_index == 0 && line.starts_with("#!"))
            || self.line_in_permitted_heredoc(cx, line_index + 1)
    }

    fn line_in_permitted_heredoc(&mut self, cx: &Investigation<'_, '_>, line_number: usize) -> bool {
        let allowed_heredoc = cx.cop_config_value("AllowHeredoc");
        let allowed: Option<Vec<&str>> = match allowed_heredoc {
            None | Some(Value::Null) | Some(Value::Bool(false)) => return false,
            Some(Value::Bool(true)) => None,
            Some(Value::Sequence(items)) => Some(items.iter().filter_map(Value::as_str).collect()),
            Some(Value::String(s)) => Some(vec![s.as_str()]),
            Some(_) => Some(Vec::new()),
        };
        self.heredocs(cx.processed_source)
            .iter()
            .any(|(range, delimiter)| {
                range.contains(&line_number)
                    && allowed
                        .as_ref()
                        .is_none_or(|list| list.contains(&delimiter.as_str()))
            })
    }

    fn register_offense(
        &mut self,
        cx: &mut Investigation<'_, '_>,
        loc: Range,
        line: &str,
        line_index: usize,
        length: usize,
    ) {
        let message = message(length, max_or_inf(cx));
        // `self.max = line_length(line)` in the block.
        if cx.offense_yields(loc) {
            let line_length = line_length(cx, line);
            cx.set_exclude_limit("Max", line_length);
        }
        let breakable_range = self.breakable_range_by_line_index.get(&line_index).copied();
        let insertion = match self.breakable_string_delimiters.get(&line_index) {
            Some(delimiter) => format!("{delimiter} \\\n{delimiter}"),
            None => "\n".to_owned(),
        };
        cx.add_offense_with_correction(loc, &message, |corrector| {
            if let Some(breakable_range) = breakable_range {
                corrector.insert_before(breakable_range, insertion);
            }
        });
    }

    fn check_directive_line(&mut self, cx: &mut Investigation<'_, '_>, line: &str, line_index: usize) {
        let length_without_directive = line_length_without_directive(line);
        let max = max_or_inf(cx);
        if length_without_directive <= max {
            return;
        }
        let range = source_range(
            cx.processed_source,
            line_index + 1,
            max,
            length_without_directive - max,
        );
        self.register_offense(cx, range, line, line_index, length_without_directive);
    }

    fn check_line_for_exemptions(&mut self, cx: &mut Investigation<'_, '_>, line: &str, line_index: usize) {
        let uri_range = allow_uri(cx)
            .then(|| find_excessive_range(cx, line, ExcessiveType::Uri))
            .flatten();
        let qualified_name_range = allow_qualified_name(cx)
            .then(|| find_excessive_range(cx, line, ExcessiveType::QualifiedName))
            .flatten();
        if allowed_combination(cx, line, uri_range.as_ref(), qualified_name_range.as_ref()) {
            return;
        }
        let range = uri_range.or(qualified_name_range);
        let loc = excess_range(cx, range, line, line_index);
        let length = line_length(cx, line);
        self.register_offense(cx, loc, line, line_index, length);
    }

    fn handle_endless_method_line(&mut self, cx: &mut Investigation<'_, '_>, line: &str, line_index: usize) {
        let node = cx
            .processed_source
            .tree()
            .node(self.endless_methods_by_line[&line_index]);
        let message = message(line_length(cx, line), max_or_inf(cx));
        let loc = excess_range(cx, None, line, line_index);
        if cx.offense_yields(loc) {
            let line_length = line_length(cx, line);
            cx.set_exclude_limit("Max", line_length);
        }
        if require_endless_methods(cx) {
            cx.add_offense_cx(loc, &message, |corrector, cx| {
                if correctable_endless_method_block(node) {
                    correct_endless_method_block_to_multiline(corrector, cx, node);
                }
            });
        } else {
            cx.add_offense_cx(loc, &message, |corrector, cx| {
                endless_method_rewriter::correct_to_multiline(
                    corrector,
                    node,
                    endless_method_rewriter::configured_indentation_width(cx),
                );
            });
        }
    }
}

fn require_endless_methods(cx: &Investigation<'_, '_>) -> bool {
    cx.config.cop_enabled_in_config("Style/EndlessMethod")
        && cx
            .config
            .for_cop("Style/EndlessMethod")
            .get("EnforcedStyle")
            .and_then(Value::as_str)
            == Some("require_always")
}

fn correctable_endless_method_block(node: Node<'_>) -> bool {
    let Some(block_node) = node.body() else {
        return false;
    };
    block_node.is_any_block()
        && block_node.has_braces()
        && block_node.is_single_line()
        && block_node.body().is_some()
        && !receiver_contains_heredoc(block_node)
}

fn correct_endless_method_block_to_multiline(
    corrector: &mut Corrector,
    _cx: &Investigation<'_, '_>,
    node: Node<'_>,
) {
    let block_node = node.body().unwrap();
    let block_arguments = match block_node.arguments_node() {
        Some(args) if block_node.has_arguments() => format!(" {}", args.source()),
        _ => String::new(),
    };
    let replacement = [
        format!("{} do{block_arguments}", block_node.send_node().unwrap().source()),
        format!("{}{}", indent(node, 2), block_node.body().unwrap().source()),
        format!("{}end", indent(node, 0)),
    ]
    .join("\n");
    corrector.replace(block_node.source_range(), replacement);
}

fn breakable_string(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("SplitStrings", false)
        && node.is_single_line()
        && !node.is_heredoc()
        && !node.parent().is_some_and(|p| {
            matches!(p.ty(), NodeType::Pair | NodeType::Kwoptarg | NodeType::Array)
        })
}

fn breakable_block_range(block_node: Node<'_>) -> Range {
    if block_node.has_arguments()
        && !block_node.is_lambda()
        && let Some(end) = block_node.arguments_node().and_then(|args| args.loc().end)
    {
        return end;
    }
    let begin = block_node.loc().begin.unwrap();
    if block_node.has_braces() {
        begin
    } else {
        begin.adjust(1, 0)
    }
}

fn breakable_range_after_semicolon(ps: &ProcessedSource<'_>, range: Range) -> Option<Range> {
    let end_pos = range.end_pos;
    let next_range = range_between(end_pos, end_pos + 1);
    let src = ps.buffer_source();
    if end_pos >= src.len() {
        return None;
    }
    if !same_line(ps, next_range, range) {
        return None;
    }
    let next_char = src[end_pos..].chars().next()?;
    if matches!(next_char, '\r' | '\n' | ';') {
        return None;
    }
    Some(range_between(end_pos, end_pos + next_char.len_utf8()))
}

fn breakable_string_position(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<usize> {
    let ps = cx.processed_source;
    let source_range = node.source_range();
    if last_column(ps, source_range) < max_or_inf(cx) {
        return None;
    }
    let pos = breakable_string_range(cx, node)?;
    (pos.end_pos != source_range.begin_pos).then_some(pos.end_pos)
}

fn breakable_string_range(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<Range> {
    let ps = cx.processed_source;
    let src = ps.buffer_source();
    let source_range = node.source_range();
    let relevant_substr = largest_possible_string(cx, node);
    let resize = |chars: usize| {
        Range::new(
            source_range.begin_pos,
            char_offset(src, source_range.begin_pos, chars),
        )
    };
    if let Some(space_pos) = breakable_space_position(cx, node, &relevant_substr) {
        Some(resize(space_pos + 1))
    } else if let Some(escape_pos) = regex!(r"\\(u[\da-f]{0,4}|x[\da-f]{0,2})?\z")
        .find(&relevant_substr)
        .map(|(begin, _)| relevant_substr[..begin].chars().count())
    {
        Some(resize(escape_pos))
    } else {
        let max = max_or_inf(cx) as isize;
        let adjustment = max - last_column(ps, source_range) as isize - 3;
        let size = src[source_range.begin_pos..source_range.end_pos].chars().count() as isize;
        if adjustment.abs() > size {
            return None;
        }
        // `source_range.adjust(end_pos: adjustment)`, in characters.
        let chars = (size + adjustment) as usize;
        Some(resize(chars))
    }
}

/// The character index of the last `\s` in `s`.
fn rindex_space(s: &str) -> Option<usize> {
    s.chars()
        .enumerate()
        .filter(|(_, c)| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
        .map(|(i, _)| i)
        .last()
}

fn breakable_space_position(cx: &Investigation<'_, '_>, node: Node<'_>, substr: &str) -> Option<usize> {
    let limit = string_content_length(cx, node) as isize - 1;
    let space_pos = rindex_space(substr)?;
    if space_pos as isize + 1 > limit {
        let limit = limit.max(0) as usize;
        let head: String = substr.chars().take(limit).collect();
        return rindex_space(&head);
    }
    Some(space_pos)
}

fn string_content_length(cx: &Investigation<'_, '_>, node: Node<'_>) -> usize {
    let source_range = node.source_range();
    let content_end = node.loc().end.map_or(source_range.end_pos, |end| end.begin_pos);
    cx.processed_source.buffer_source()[source_range.begin_pos..content_end.max(source_range.begin_pos)]
        .chars()
        .count()
}

fn breakable_dstr_begin_position(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<usize> {
    let ps = cx.processed_source;
    let source_range = node.source_range();
    let max = max_or_inf(cx);
    (ps.column_of(source_range.begin_pos) < max && last_column(ps, source_range) >= max)
        .then_some(source_range.begin_pos)
}

fn extract_heredocs(ps: &ProcessedSource<'_>) -> Vec<(CharRange<usize>, String)> {
    let Some(ast) = ps.ast() else {
        return Vec::new();
    };
    ast.each_node()
        .filter(|n| matches!(n.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr) && n.is_heredoc())
        .filter_map(|node| {
            let body = node.loc().heredoc_body?;
            let delimiter = node.range_source(node.loc().heredoc_end?).trim().to_owned();
            Some((ps.line_of(body.begin_pos)..ps.line_of(body.end_pos), delimiter))
        })
        .collect()
}

fn receiver_contains_heredoc(node: Node<'_>) -> bool {
    let Some(receiver) = node.receiver() else {
        return false;
    };
    let is_str_heredoc = |n: Node<'_>| {
        matches!(n.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr) && n.is_heredoc()
    };
    is_str_heredoc(receiver) || receiver.descendants().any(is_str_heredoc)
}

fn excess_range(
    cx: &Investigation<'_, '_>,
    uri_range: Option<CharRange<usize>>,
    line: &str,
    line_index: usize,
) -> Range {
    let max = max_or_inf(cx);
    let excessive_position = match uri_range {
        Some(uri_range) if uri_range.start < max => uri_range.end,
        _ => highlight_start(cx, line),
    };
    let length = line_length(cx, line).saturating_sub(excessive_position);
    source_range(cx.processed_source, line_index + 1, excessive_position, length)
}

fn highlight_start(cx: &Investigation<'_, '_>, line: &str) -> usize {
    max_or_inf(cx).saturating_sub(indentation_difference(cx, line))
}

fn allowed_combination(
    cx: &Investigation<'_, '_>,
    line: &str,
    uri_range: Option<&CharRange<usize>>,
    qualified_name_range: Option<&CharRange<usize>>,
) -> bool {
    match (uri_range, qualified_name_range) {
        (Some(uri), Some(qn)) => allowed_position(cx, line, uri) && allowed_position(cx, line, qn),
        (Some(uri), None) => allowed_position(cx, line, uri),
        (None, Some(qn)) => allowed_position(cx, line, qn),
        (None, None) => false,
    }
}

fn breakable_dstr(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    breakable_string(cx, node) && node.child_nodes().count() != 1
}

fn string_delimiter(node: Node<'_>) -> Option<&str> {
    let delimiter = if let Some(begin) = node.loc().begin {
        Some(node.range_source(begin))
    } else {
        node.parent()
            .filter(|p| p.is(NodeType::Dstr))
            .and_then(|p| p.loc().begin.map(|b| p.range_source(b)))
    }?;
    matches!(delimiter, "'" | "\"").then_some(delimiter)
}

fn largest_possible_string(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let ps = cx.processed_source;
    let mut max_length = max_or_inf(cx) as isize - 3;
    max_length -= match node.parent() {
        Some(parent) if same_line(ps, node.source_range(), parent.source_range()) => {
            column_offset_between(ps, node.source_range(), parent.source_range())
        }
        _ => node.column() as isize,
    };
    let source = node.source();
    // `source[0...max_length]`: a negative end counts from the end.
    let count = source.chars().count() as isize;
    let take = if max_length < 0 {
        (count + max_length).max(0)
    } else {
        max_length.min(count)
    };
    source.chars().take(take as usize).collect()
}
