//! Port of `lib/rubocop/cop/style/document_dynamic_eval_definition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ruby_regex;
use crate::cop::mixin::util::regexp_escape;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;
use crate::regex;

const MSG: &str = "Add a comment block showing its appearance if interpolated.";

fn block_comment_regexp() -> &'static onig::Regex {
    regex!(r"^\s*#(?!{)")
}

fn comment_regexp() -> &'static onig::Regex {
    regex!(r"\s*#(?!{).*")
}

#[derive(Default)]
pub struct DocumentDynamicEvalDefinition;

impl Cop for DocumentDynamicEvalDefinition {
    fn name(&self) -> &'static str {
        "Style/DocumentDynamicEvalDefinition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["eval", "class_eval", "module_eval", "instance_eval"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(arg_node) = node
            .first_argument()
            .filter(|a| a.is(NodeType::Dstr) && interpolated(*a))
        else {
            return;
        };
        let ps = cx.processed_source;
        if inline_comment_docs(ps, arg_node)
            || (arg_node.is_heredoc() && comment_block_docs(ps, arg_node))
        {
            return;
        }
        cx.add_offense(node.loc().selector.unwrap(), MSG);
    }
}

fn interpolated(arg_node: Node<'_>) -> bool {
    arg_node.child_nodes().any(|c| c.is(NodeType::Begin))
}

fn inline_comment_docs(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    node.child_nodes()
        .filter(|c| c.is(NodeType::Begin))
        .all(|begin_node| {
            let source_line = ps
                .lines()
                .get(begin_node.first_line() - 1)
                .copied()
                .unwrap_or("");
            comment_regexp().find(source_line).is_some()
        })
}

fn comment_block_docs(ps: &ProcessedSource<'_>, arg_node: Node<'_>) -> bool {
    let body = arg_node.loc().heredoc_body.unwrap();
    let mut comments = heredoc_comment_blocks(
        ps,
        ps.line_of(body.begin_pos),
        ps.line_of(body.end_pos),
    );
    comments.extend(preceding_comment_blocks(ps, arg_node.parent().unwrap()));
    if comments.is_empty() {
        return false;
    }
    let Some(regexp) = ruby_regex(&comment_regexp_source(arg_node)) else {
        return false;
    };
    comments.iter().any(|c| regexp.find(c).is_some()) || regexp.find(&comments.concat()).is_some()
}

fn preceding_comment_blocks(ps: &ProcessedSource<'_>, node: Node<'_>) -> Vec<String> {
    let mut hash: Vec<(usize, String)> = Vec::new();
    for comment in ps.comments_in_lines(node.first_line(), node.last_line()) {
        merge_adjacent_comments(comment.text, ps.line_of(comment.range.begin_pos), &mut hash);
    }
    hash.into_iter().map(|(_, v)| v).collect()
}

fn heredoc_comment_blocks(ps: &ProcessedSource<'_>, first: usize, last: usize) -> Vec<String> {
    let mut hash: Vec<(usize, String)> = Vec::new();
    let lines = ps.lines();
    let begin = first.saturating_sub(1);
    let end = (last.saturating_sub(1) + 1).min(lines.len());
    if begin < end {
        for (offset, line) in lines[begin..end].iter().enumerate() {
            merge_adjacent_comments(line, begin + offset, &mut hash);
        }
    }
    hash.into_iter().map(|(_, v)| v).collect()
}

fn merge_adjacent_comments(line: &str, index: usize, hash: &mut Vec<(usize, String)>) {
    if block_comment_regexp().find(line).is_none() {
        return;
    }
    let line = block_comment_regexp().replace_all(line, "");
    if index > 0 && hash.last().is_some_and(|(k, _)| *k == index - 1) {
        let (_, previous) = hash.pop().unwrap();
        hash.push((index, format!("{previous}\n{line}")));
    } else {
        hash.push((index, line));
    }
}

/// `Regexp#to_s` of a regexp without options.
fn regexp_to_s(source: &str) -> String {
    format!("(?-mix:{source})")
}

fn comment_regexp_source(arg_node: Node<'_>) -> String {
    arg_node
        .child_nodes()
        .map(|n| {
            if n.is(NodeType::Begin) {
                regexp_to_s(".+")
            } else {
                source_to_regexp(n.source()).unwrap_or_default()
            }
        })
        .collect()
}

fn source_to_regexp(source: &str) -> Option<String> {
    if source.trim().is_empty() {
        return Some(regexp_to_s(r"\s+"));
    }
    let source = comment_regexp().replace_all(source, "");
    if source.trim().is_empty() {
        return None;
    }
    let segments: Vec<String> = source
        .trim()
        .split("\\#")
        .map(regexp_escape)
        .collect();
    Some(regexp_to_s(&format!(r"\s*{}", segments.join(r"\\?#"))))
}

