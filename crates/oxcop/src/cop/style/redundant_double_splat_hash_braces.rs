//! Port of `lib/rubocop/cop/style/redundant_double_splat_hash_braces.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Remove the redundant double splat and braces, use keyword arguments directly.";
const MERGE_METHODS: &[&str] = &["merge", "merge!"];

#[derive(Default)]
pub struct RedundantDoubleSplatHashBraces;

impl Cop for RedundantDoubleSplatHashBraces {
    fn name(&self) -> &'static str {
        "Style/RedundantDoubleSplatHashBraces"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let pairs = node.pairs();
        if pairs.is_empty() || pairs.iter().any(|p| p.is_hash_rocket()) {
            return;
        }
        let Some(parent) = node.parent() else {
            return;
        };
        if !(parent.is_call() || parent.is(NodeType::Kwsplat)) || !mergeable(parent) {
            return;
        }
        let Some(kwsplat) = node.ancestors().find(|a| a.is(NodeType::Kwsplat)) else {
            return;
        };
        if !node.has_braces() || allowed_double_splat_receiver(kwsplat) {
            return;
        }
        cx.add_offense_with_correction(kwsplat.source_range(), MSG, |corrector| {
            autocorrect(corrector, node, kwsplat);
        });
    }
}

fn allowed_double_splat_receiver(kwsplat: Node<'_>) -> bool {
    let Some(first_child) = kwsplat.child(0) else {
        return false;
    };
    if first_child.is_any_block() {
        return true;
    }
    if !first_child.is_call() {
        return false;
    }
    !root_receiver(first_child).is_some_and(|r| r.is(NodeType::Hash))
}

fn root_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver();
    match receiver {
        Some(r) if r.receiver().is_some() => root_receiver(r),
        _ => receiver,
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, kwsplat: Node<'_>) {
    corrector.remove(kwsplat.loc().operator.unwrap());
    corrector.remove(
        node.loc()
            .begin
            .unwrap()
            .join(node.child(0).unwrap().source_range().begin()),
    );
    corrector.remove(
        node.child_from_end(1)
            .unwrap()
            .source_range()
            .end()
            .join(node.loc().end.unwrap()),
    );
    let calls: Vec<Node<'_>> = kwsplat.descendants().filter(|d| d.is_call()).collect();
    let merge_methods: Vec<Node<'_>> = calls.iter().copied().filter(|c| mergeable(*c)).collect();
    if merge_methods.is_empty() {
        return;
    }
    let range = range_of_merge_methods(&merge_methods);
    let mut new_arguments: Vec<Vec<String>> = calls
        .iter()
        .filter_map(|d| convert_to_new_arguments(*d))
        .collect();
    new_arguments.reverse();
    let mut parts = vec![String::new()];
    parts.extend(new_arguments.into_iter().flatten());
    corrector.replace(range, parts.join(", "));
}

fn range_of_merge_methods(merge_methods: &[Node<'_>]) -> Range {
    let begin_merge_method = merge_methods.last().unwrap();
    let end_merge_method = merge_methods.first().unwrap();
    begin_merge_method
        .loc()
        .dot
        .unwrap()
        .begin()
        .join(end_merge_method.source_range().end())
}

fn convert_to_new_arguments(node: Node<'_>) -> Option<Vec<String>> {
    if !mergeable(node) {
        return None;
    }
    Some(
        node.arguments()
            .into_iter()
            .map(|arg| {
                if arg.is(NodeType::Hash) {
                    hash_argument_source(arg)
                } else {
                    format!("**{}", arg.source())
                }
            })
            .collect(),
    )
}

fn mergeable(node: Node<'_>) -> bool {
    if !node.is_call() {
        return true;
    }
    if !node
        .method_name()
        .is_some_and(|m| MERGE_METHODS.contains(&m))
    {
        return false;
    }
    match node.parent() {
        Some(parent) => mergeable(parent),
        None => true,
    }
}

fn hash_argument_source(hash: Node<'_>) -> String {
    if !hash.has_braces() {
        return hash.source().to_owned();
    }
    hash.pairs()
        .iter()
        .map(|p| p.source())
        .collect::<Vec<_>>()
        .join(", ")
}
