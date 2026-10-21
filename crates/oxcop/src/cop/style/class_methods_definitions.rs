//! Port of `lib/rubocop/cop/style/class_methods_definitions.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::source_range_with_comment;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::visibility_help::node_visibility;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG_SCLASS: &str = "Do not define public methods within class << self.";

#[derive(Default)]
pub struct ClassMethodsDefinitions;

impl Cop for ClassMethodsDefinitions {
    fn name(&self) -> &'static str {
        "Style/ClassMethodsDefinitions"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Sclass, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let def_self_style = style(cx) == "def_self";
        if node.is(NodeType::Sclass) {
            if !def_self_style
                || !node.identifier().is_some_and(|i| i.is(NodeType::SelfNode))
                || !all_methods_public(node)
            {
                return;
            }
            let ps = cx.processed_source;
            cx.add_offense_with_correction(node.source_range(), MSG_SCLASS, |corrector| {
                autocorrect_sclass(ps, node, corrector);
            });
        } else {
            if def_self_style || !node.receiver().is_some_and(|r| r.is(NodeType::SelfNode)) {
                return;
            }
            cx.add_offense(
                node.source_range(),
                "Use `class << self` to define a class method.",
            );
        }
    }
}

fn all_methods_public(sclass: Node<'_>) -> bool {
    let defs = def_nodes(sclass);
    !defs.is_empty() && defs.iter().all(|d| node_visibility(*d) == "public")
}

fn def_nodes(sclass: Node<'_>) -> Vec<Node<'_>> {
    let Some(body) = sclass.body() else {
        return Vec::new();
    };
    if body.is(NodeType::Def) {
        vec![body]
    } else if body.is(NodeType::Begin) {
        body.child_nodes().filter(|c| c.is(NodeType::Def)).collect()
    } else {
        Vec::new()
    }
}

fn autocorrect_sclass(ps: &ProcessedSource<'_>, node: Node<'_>, corrector: &mut Corrector) {
    let mut rewritten_defs: Vec<String> = Vec::new();
    for def_node in def_nodes(node) {
        if node_visibility(def_node) != "public" {
            continue;
        }
        let (range, source) = extract_def_from_sclass(ps, def_node, node);
        corrector.remove(range);
        rewritten_defs.push(source);
    }
    if sclass_only_has_methods(node) {
        corrector.remove(node.source_range());
        if let Some(first) = rewritten_defs.first_mut() {
            *first = first.trim().to_owned();
        }
    } else {
        corrector.insert_after(node.source_range(), "\n");
    }
    corrector.insert_after(node.source_range(), rewritten_defs.join("\n"));
}

fn sclass_only_has_methods(node: Node<'_>) -> bool {
    let body = node.body().unwrap();
    body.is(NodeType::Def) || body.child_nodes().all(|c| c.is(NodeType::Def))
}

fn extract_def_from_sclass(
    ps: &ProcessedSource<'_>,
    def_node: Node<'_>,
    sclass: Node<'_>,
) -> (Range, String) {
    let range = source_range_with_comment(ps, def_node);
    let source = prefix_def_with_self(ps, range, def_node);
    let diff = def_node.column() as isize - sclass.column() as isize;
    let indent = " ".repeat(diff.max(0) as usize);
    let source: String = source
        .split_inclusive('\n')
        .map(|line| line.strip_prefix(indent.as_str()).unwrap_or(line))
        .collect();
    let source = source
        .strip_suffix('\n')
        .map_or(source.clone(), str::to_owned);
    (range, source)
}

fn prefix_def_with_self(ps: &ProcessedSource<'_>, range: Range, def_node: Node<'_>) -> String {
    let keyword_offset = def_node.loc().keyword.unwrap().begin_pos - range.begin_pos;
    let name_end_offset = def_node.loc().name.unwrap().end_pos - range.begin_pos;
    let source = ps.slice(range);
    format!(
        "{}def self.{}{}",
        &source[..keyword_offset],
        def_node.method_name().unwrap_or(""),
        &source[name_end_offset..]
    )
}
