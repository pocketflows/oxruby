//! Port of `lib/rubocop/cop/metrics/utils/code_length_calculator.rb`.

use std::collections::BTreeSet;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::comment_line;
use crate::processed_source::ProcessedSource;

/// The `CountAsOne` types.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Foldable {
    Array,
    Hash,
    Heredoc,
    MethodCall,
}

impl Foldable {
    pub fn from_name(name: &str) -> Option<Foldable> {
        match name {
            "array" => Some(Foldable::Array),
            "hash" => Some(Foldable::Hash),
            "heredoc" => Some(Foldable::Heredoc),
            "method_call" => Some(Foldable::MethodCall),
            _ => None,
        }
    }

    fn check(self, node: Node<'_>) -> bool {
        match self {
            Foldable::Array => node.is(NodeType::Array),
            Foldable::Hash => node.is(NodeType::Hash),
            Foldable::Heredoc => heredoc_node(node),
            Foldable::MethodCall => node.is_call(),
        }
    }

    /// `normalize_foldable_types`.
    fn types(self) -> &'static [NodeType] {
        match self {
            Foldable::Array => &[NodeType::Array],
            Foldable::Hash => &[NodeType::Hash],
            Foldable::Heredoc => &[NodeType::Str, NodeType::Dstr],
            Foldable::MethodCall => &[NodeType::Send, NodeType::Csend],
        }
    }
}

/// Helps to calculate code length for the provided node.
pub struct CodeLengthCalculator<'a, 'p, 'src> {
    node: Node<'a>,
    processed_source: &'p ProcessedSource<'src>,
    count_comments: bool,
    foldable_types: Vec<Foldable>,
}

impl<'a, 'p, 'src> CodeLengthCalculator<'a, 'p, 'src> {
    pub fn new(
        node: Node<'a>,
        processed_source: &'p ProcessedSource<'src>,
        count_comments: bool,
        foldable_types: Vec<Foldable>,
    ) -> Self {
        Self {
            node,
            processed_source,
            count_comments,
            foldable_types,
        }
    }

    pub fn calculate(&self) -> usize {
        let mut length = self.code_length(self.node) as isize;
        if self.foldable_types.is_empty() {
            return length as usize;
        }
        let types: Vec<NodeType> = self
            .foldable_types
            .iter()
            .flat_map(|f| f.types().iter().copied())
            .collect();
        let mut descendants = Vec::new();
        each_top_level_descendant(self.node, &types, &mut descendants);
        for descendant in descendants {
            if !self.foldable_node(descendant) {
                continue;
            }
            let descendant_length = self.code_length(descendant) as isize;
            length = length - descendant_length + 1;
            // Subtract length of opening and closing brace if method argument
            // omits hash braces.
            if descendant.is(NodeType::Hash) && !descendant.has_braces() {
                length -= omit_length(descendant) as isize;
            }
        }
        length.max(0) as usize
    }

    fn code_length(&self, node: Node<'_>) -> usize {
        if classlike_node(Some(node)) {
            self.classlike_code_length(node)
        } else if heredoc_node(node) {
            self.heredoc_length(node)
        } else {
            let Some(body) = extract_body(node) else {
                return 0;
            };
            if node_with_heredoc(body) {
                self.source_from_node_with_heredoc(body)
                    .iter()
                    .filter(|line| !self.irrelevant_line(line))
                    .count()
            } else {
                body.source()
                    .split_inclusive('\n')
                    .filter(|line| !self.irrelevant_line(line))
                    .count()
            }
        }
    }

    fn classlike_code_length(&self, node: Node<'_>) -> usize {
        if namespace_module(node) {
            return 0;
        }
        let (first, last) = (node.first_line(), node.last_line());
        let inner = line_numbers_of_inner_nodes(node);
        let lines = self.processed_source.lines();
        ((first + 1)..last)
            .filter(|line_number| !inner.contains(line_number))
            // `@processed_source[line_number]` indexes the 0-based `lines`.
            .filter(|&line_number| {
                lines
                    .get(line_number)
                    .is_some_and(|line| !self.irrelevant_line(line))
            })
            .count()
    }

    fn heredoc_length(&self, node: Node<'_>) -> usize {
        let body = node.loc().heredoc_body.unwrap();
        node.range_source(body)
            .split_inclusive('\n')
            .filter(|line| !self.irrelevant_line(line))
            .count()
            + 2
    }

    fn foldable_node(&self, node: Node<'_>) -> bool {
        self.foldable_types.iter().any(|f| f.check(node))
    }

    /// Returns true for lines that shall not be included in the count.
    fn irrelevant_line(&self, source_line: &str) -> bool {
        blank(source_line) || (!self.count_comments && comment_line(source_line))
    }

    fn source_from_node_with_heredoc(&self, node: Node<'_>) -> Vec<&'src str> {
        let mut last_line = 0;
        for descendant in node.descendants() {
            if !descendant.has_source() {
                continue;
            }
            let descendant_last_line = if heredoc_node(descendant) {
                descendant.line_of(descendant.loc().heredoc_end.unwrap())
            } else {
                descendant.last_line()
            };
            last_line = last_line.max(descendant_last_line);
        }
        let lines = self.processed_source.lines();
        let first = node.first_line() - 1;
        if last_line == 0 || last_line - 1 < first {
            return Vec::new();
        }
        lines[first..last_line.min(lines.len())].to_vec()
    }
}

/// rubocop's `String#blank?`.
pub fn blank(line: &str) -> bool {
    line.trim_start_matches(['\0', '\t', '\n', '\x0b', '\x0c', '\r', ' '])
        .is_empty()
}

fn heredoc_node(node: Node<'_>) -> bool {
    node.is_heredoc()
}

fn line_numbers_of_inner_nodes(node: Node<'_>) -> BTreeSet<usize> {
    node.descendants()
        .filter(|d| matches!(d.ty(), NodeType::Module | NodeType::Class))
        .flat_map(|inner| inner.first_line()..=inner.last_line())
        .collect()
}

fn namespace_module(node: Node<'_>) -> bool {
    classlike_node(node.body())
}

fn classlike_node(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| matches!(n.ty(), NodeType::Class | NodeType::Module))
}

fn each_top_level_descendant<'a>(node: Node<'a>, types: &[NodeType], out: &mut Vec<Node<'a>>) {
    for child in node.child_nodes() {
        if classlike_node(Some(child)) {
            continue;
        }
        if types.contains(&child.ty()) {
            out.push(child);
        } else {
            each_top_level_descendant(child, types, out);
        }
    }
}

fn extract_body(node: Node<'_>) -> Option<Node<'_>> {
    use NodeType::*;
    match node.ty() {
        Class | Module | Sclass | Block | Numblock | Itblock | Def | Defs => node.body(),
        Casgn => node.expression().and_then(extract_body),
        _ => Some(node),
    }
}

fn omit_length(descendant: Node<'_>) -> usize {
    let Some(parent) = descendant.parent() else {
        return 0;
    };
    if parent.is_call() && parent.arguments().len() > 1 {
        return 0;
    }
    if !(parent.is_call() && parent.is_parenthesized()) {
        return 0;
    }
    let range = descendant.source_range();
    [
        parent.loc().begin.unwrap().end_pos != range.begin_pos,
        parent.loc().end.unwrap().begin_pos != range.end_pos,
    ]
    .iter()
    .filter(|&&b| b)
    .count()
}

fn node_with_heredoc(node: Node<'_>) -> bool {
    node.descendants()
        .any(|d| matches!(d.ty(), NodeType::Str | NodeType::Dstr) && heredoc_node(d))
}
