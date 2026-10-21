//! Port of `lib/rubocop/cop/style/string_concatenation.rb`.

use std::collections::HashSet;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Prefer string interpolation to string concatenation.";

static STRING_CONCATENATION: NodePattern = NodePattern::new(
    "{
       (send str_type? :+ _)
       (send _ :+ str_type?)
     }",
);

#[derive(Default)]
pub struct StringConcatenation {
    corrected_nodes: HashSet<NodeId>,
}

impl Cop for StringConcatenation {
    fn name(&self) -> &'static str {
        "Style/StringConcatenation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["+"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.corrected_nodes.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !STRING_CONCATENATION.is_match(node) || line_end_concatenation(node) {
            return;
        }
        let topmost_plus_node = find_topmost_plus_node(node);
        let mut parts = Vec::new();
        collect_parts(Some(topmost_plus_node), &mut parts);
        if cx.cop_config_str("Mode") == Some("conservative")
            && !parts.first().is_some_and(|p| p.is(NodeType::Str))
        {
            return;
        }
        let correctable_parts = !parts.iter().any(|part| uncorrectable(*part));
        let corrected_ancestor = topmost_plus_node
            .ancestors()
            .filter(|a| a.is_send())
            .any(|a| self.corrected_nodes.contains(&a.id()));
        let mut corrected = false;
        cx.add_offense_with_correction(topmost_plus_node.source_range(), MSG, |corrector| {
            if correctable_parts && !corrected_ancestor {
                corrector.replace(topmost_plus_node.source_range(), replacement(&parts));
                corrected = true;
            }
        });
        if corrected {
            self.corrected_nodes.insert(topmost_plus_node.id());
        }
    }
}

fn line_end_concatenation(node: Node<'_>) -> bool {
    node.receiver().is_some_and(|r| r.is(NodeType::Str))
        && node.first_argument().is_some_and(|a| a.is(NodeType::Str))
        && node.is_multiline()
        && regex!(r"\+\s*\n").find(node.source()).is_some()
}

fn find_topmost_plus_node(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if !plus_node(parent) {
            break;
        }
        current = parent;
    }
    current
}

fn collect_parts<'a>(node: Option<Node<'a>>, parts: &mut Vec<Node<'a>>) {
    let Some(node) = node else {
        return;
    };
    if plus_node(node) {
        collect_parts(node.receiver(), parts);
        collect_parts(node.first_argument(), parts);
    } else {
        parts.push(node);
    }
}

fn plus_node(node: Node<'_>) -> bool {
    node.is_send() && node.is_method("+")
}

fn uncorrectable(part: Node<'_>) -> bool {
    part.is_multiline() || heredoc(part) || part.descendants().any(|d| d.is_any_block())
}

fn heredoc(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::Str, NodeType::Dstr]) && node.is_heredoc()
}

fn replacement(parts: &[Node<'_>]) -> String {
    let interpolated: Vec<String> = parts.iter().map(|part| adjust_str(*part)).collect();
    let quoted: Vec<&str> = interpolated
        .iter()
        .map(|part| if part == "\"" { "\\\"" } else { part.as_str() })
        .collect();
    format!("\"{}\"", quoted.join(""))
}

fn adjust_str(part: Node<'_>) -> String {
    match part.ty() {
        NodeType::Str => adjust_str_literal(part),
        NodeType::Dstr | NodeType::Begin => part.child_nodes().map(adjust_str).collect(),
        _ => format!("#{{{}}}", part.source()),
    }
}

fn adjust_str_literal(part: Node<'_>) -> String {
    let value = part.str_content().unwrap_or_default();
    if part.source().starts_with('\'') {
        regex!(r#"(\\|"|#\{|#@|#\$)"#).replace_all(value, |caps: &onig::Captures<'_>| {
            format!("\\{}", caps.at(0).unwrap_or(""))
        })
    } else if part.source().starts_with('"') {
        let source = part.source();
        source[1..source.len() - 1].to_owned()
    } else {
        let inspected = ruby_inspect(value);
        inspected[1..inspected.len() - 1].to_owned()
    }
}
