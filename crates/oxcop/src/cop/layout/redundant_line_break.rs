//! Port of `lib/rubocop/cop/layout/redundant_line_break.rb`.
//!
//! `normalize_reparsed_ast` (`fold_string_concatenation`) builds updated
//! nodes in Ruby; here both trees are folded into a [`Folded`] value that is
//! compared instead.

use std::borrow::Cow;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::check_single_line_suitability::{suitable_as_single_line, to_single_line};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::reparsed_equivalence::{Oversized, ReparsedEquivalence, verified_by_reparse};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;
use crate::regex;

const MSG: &str = "Redundant line break detected.";

#[derive(Default)]
pub struct RedundantLineBreak {
    ignored_nodes: IgnoredNodes,
}

impl Cop for RedundantLineBreak {
    fn name(&self) -> &'static str {
        "Layout/RedundantLineBreak"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        // `CheckAssignment`'s callbacks, plus the cop's `on_csend`.
        &[
            NodeType::Lvasgn,
            NodeType::Ivasgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
            NodeType::Casgn,
            NodeType::Masgn,
            NodeType::OpAsgn,
            NodeType::OrAsgn,
            NodeType::AndAsgn,
            NodeType::Send,
            NodeType::Csend,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_call() {
            self.on_send(cx, node);
            return;
        }
        if node.is(NodeType::Lvasgn) && end_with_percent_blank_string(cx) {
            return;
        }
        // `check_assignment(node, _rhs)`.
        if !offense(cx, node) {
            return;
        }
        self.register_offense(cx, node);
    }
}

impl RedundantLineBreak {
    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let mut node = node;
        while let Some(parent) = node.parent() {
            if parent.is(NodeType::Send)
                || convertible_block(node)
                || matches!(parent.ty(), NodeType::And | NodeType::Or)
            {
                node = parent;
            } else {
                break;
            }
        }
        if !offense(cx, node) || self.ignored_nodes.part_of(node) {
            return;
        }
        self.register_offense(cx, node);
    }

    fn register_offense<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if verified_by_reparse(self, cx, &[node], Oversized::Verify).is_empty() {
            return;
        }
        let replacement = to_single_line(node.source()).trim().to_owned();
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), replacement);
        });
        self.ignored_nodes.ignore(node);
    }
}

impl<'n> ReparsedEquivalence<Node<'n>> for RedundantLineBreak {
    fn apply_reparse_correction(
        &self,
        _cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        node: Node<'n>,
    ) {
        corrector.replace(node.source_range(), to_single_line(node.source()).trim());
    }

    fn item_range(&self, node: Node<'n>) -> Range {
        node.source_range()
    }

    fn preprocess_reparsed_source<'s>(&self, source: &'s str) -> Cow<'s, str> {
        let re = regex!(r"\b__LINE__\b");
        if re.find(source).is_none() {
            return Cow::Borrowed(source);
        }
        Cow::Owned(re.replace_all(source, |_: &onig::Captures<'_>| "__LINE0__".to_owned()))
    }

    fn reparsed_equal(&self, original: Node<'_>, rewritten: Node<'_>) -> bool {
        fold_string_concatenation(original) == fold_string_concatenation(rewritten)
    }
}

/// A node after `fold_string_concatenation`, for comparison.
#[derive(Debug, Clone, PartialEq)]
enum Folded {
    Node(NodeType, Vec<Folded>),
    Value(Child),
}

impl Folded {
    fn is_type(&self, ty: NodeType) -> bool {
        matches!(self, Folded::Node(t, _) if *t == ty)
    }

    fn children(&self) -> &[Folded] {
        match self {
            Folded::Node(_, children) => children,
            Folded::Value(_) => &[],
        }
    }
}

fn fold_string_concatenation(node: Node<'_>) -> Folded {
    let children: Vec<Folded> = node
        .children()
        .iter()
        .map(|child| match child {
            Child::Node(id) => fold_string_concatenation(node.ast().node(*id)),
            other => Folded::Value(other.clone()),
        })
        .collect();
    let folded = Folded::Node(node.ty(), children);
    let Some(parts) = string_concatenation_parts(node, &folded) else {
        return folded;
    };
    let merged = merge_string_parts(parts);
    if merged.len() == 1 && merged[0].is_type(NodeType::Str) {
        Folded::Node(NodeType::Str, merged[0].children().to_vec())
    } else {
        Folded::Node(NodeType::Dstr, merged)
    }
}

fn string_concatenation_parts(node: Node<'_>, folded: &Folded) -> Option<Vec<Folded>> {
    match node.ty() {
        NodeType::Dstr => Some(folded.children().to_vec()),
        NodeType::Send => {
            let children = folded.children();
            // `[node.receiver, *node.arguments]`.
            let mut parts = vec![children[0].clone()];
            parts.extend(children.iter().skip(2).cloned());
            (node.is_method("+") && parts.iter().all(stringish)).then_some(parts)
        }
        _ => None,
    }
}

fn merge_string_parts(parts: Vec<Folded>) -> Vec<Folded> {
    let flattened: Vec<Folded> = parts
        .into_iter()
        .flat_map(|part| {
            if part.is_type(NodeType::Dstr) {
                part.children().to_vec()
            } else {
                vec![part]
            }
        })
        .collect();
    let mut chunks: Vec<Vec<Folded>> = Vec::new();
    for part in flattened {
        match chunks.last_mut() {
            Some(chunk) if plain_string(chunk.last().unwrap()) && plain_string(&part) => {
                chunk.push(part);
            }
            _ => chunks.push(vec![part]),
        }
    }
    chunks.into_iter().map(merge_plain_strings).collect()
}

fn merge_plain_strings(mut chunk: Vec<Folded>) -> Folded {
    if chunk.len() == 1 {
        return chunk.pop().unwrap();
    }
    let mut bytes = Vec::new();
    for part in &chunk {
        if let Some(Folded::Value(value)) = part.children().first()
            && let Some(b) = value.str_bytes()
        {
            bytes.extend_from_slice(b);
        }
    }
    Folded::Node(NodeType::Str, vec![Folded::Value(Child::string(bytes))])
}

fn stringish(node: &Folded) -> bool {
    node.is_type(NodeType::Str) || node.is_type(NodeType::Dstr)
}

fn plain_string(node: &Folded) -> bool {
    node.is_type(NodeType::Str) && node.children().len() == 1
}

fn end_with_percent_blank_string(cx: &Investigation<'_, '_>) -> bool {
    cx.processed_source.buffer_source().ends_with("%\n\n")
}

fn offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !(node.is_multiline() && suitable_as_single_line(cx, node)) {
        return false;
    }
    if node.is_operator_keyword() {
        return require_backslash(cx, node);
    }
    !index_access_call_chained(node) && !configured_to_not_be_inspected(cx, node)
}

fn require_backslash(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let line = node.line_of(node.loc().operator.unwrap());
    cx.processed_source.lines()[line - 1].ends_with('\\')
}

fn index_access_call_chained(node: Node<'_>) -> bool {
    if !(node.is_send() && node.is_method("[]")) {
        return false;
    }
    node.child(0)
        .is_some_and(|first| first.is_send() && first.is_method("[]"))
}

fn configured_to_not_be_inspected(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if other_cop_takes_precedence(cx, node) {
        return true;
    }
    if cx.cop_config_bool("InspectBlocks", false) {
        return false;
    }
    node.is_any_block()
        || node
            .descendants()
            .any(|d| d.is_any_block() && d.is_multiline())
}

fn other_cop_takes_precedence(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.config.cop_enabled_in_config("Layout/SingleLineBlockChain")
        && node.descendants().any(|block_node| {
            if !block_node.is_any_block() {
                return false;
            }
            let Some(parent) = block_node.parent() else {
                return false;
            };
            parent.is_call() && parent.loc().dot.is_some() && block_node.is_single_line()
        })
}

fn convertible_block(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is_any_block()
        && parent.send_node() == Some(node)
        && (node.is_parenthesized() || !node.has_arguments())
}
