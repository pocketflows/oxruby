//! Port of `lib/rubocop/cop/style/inverse_methods.rb`.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;
use crate::regex;

const CLASS_COMPARISON_METHODS: &[&str] = &["<=", ">=", "<", ">"];
const SAFE_NAVIGATION_INCOMPATIBLE_METHODS: &[&str] = &["<=", ">=", "<", ">", "any?", "none?"];
const EQUALITY_METHODS: &[&str] = &["==", "!=", "=~", "!~", "<=", ">=", "<", ">"];
const NEGATED_EQUALITY_METHODS: &[&str] = &["!=", "!~"];

static INVERSE_CANDIDATE: NodePattern = NodePattern::new(
    "{
      (send $(call $(...) $_ $...) :!)
      (send (any_block $(call $(...) $_) $...) :!)
      (send (begin $(call $(...) $_ $...)) :!)
    }",
);
static INVERSE_BLOCK: NodePattern = NodePattern::new(
    "(any_block $(call (...) $_) ... { $(call ... :!)
                                      $(send (...) {:!= :!~} ...)
                                      (begin ... $(call ... :!))
                                      (begin ... $(send (...) {:!= :!~} ...))
                                    })",
);

#[derive(Default)]
pub struct InverseMethods {
    ignored: IgnoredNodes,
}

impl Cop for InverseMethods {
    fn name(&self) -> &'static str {
        "Style/InverseMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/Not", "Style/SymbolProc"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Send,
            NodeType::Csend,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_call() {
            self.on_send(cx, node);
        } else {
            self.on_block(cx, node);
        }
    }
}

/// `cop_config[key].merge(cop_config[key].invert)`, with the symbols as
/// names.
fn inverse_map(cx: &Investigation<'_, '_>, key: &str) -> Vec<(String, String)> {
    let Some(Value::Mapping(map)) = cx.cop_config_value(key) else {
        return Vec::new();
    };
    let name = |v: &Value| v.as_str().map(|s| s.strip_prefix(':').unwrap_or(s).to_owned());
    let pairs: Vec<(String, String)> = map
        .iter()
        .filter_map(|(k, v)| Some((name(k)?, name(v)?)))
        .collect();
    let mut merged: Vec<(String, String)> = pairs.clone();
    for (k, v) in pairs {
        let (k, v) = (v, k);
        if let Some(entry) = merged.iter_mut().find(|(key, _)| *key == k) {
            entry.1 = v;
        } else {
            merged.push((k, v));
        }
    }
    merged
}

fn lookup<'m>(map: &'m [(String, String)], method: &str) -> Option<&'m str> {
    map.iter()
        .find(|(k, _)| k == method)
        .map(|(_, v)| v.as_str())
}

impl InverseMethods {
    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = INVERSE_CANDIDATE.match_node(node) else {
            return;
        };
        let method_call = captures[0].node().unwrap();
        let lhs = captures[1].node().unwrap();
        let Some(method) = captures[2].sym() else {
            return;
        };
        let rhs = captures[3].nodes();
        let inverse_methods = inverse_map(cx, "InverseMethods");
        let Some(inverse) = lookup(&inverse_methods, method) else {
            return;
        };
        if negated(node) || safe_navigation_incompatible(method_call) {
            return;
        }
        if self.ignored.part_of(node) {
            return;
        }
        if possible_class_hierarchy_check(lhs, &rhs, method) {
            return;
        }
        let message = format!("Use `{inverse}` instead of inverting `{method}`.");
        let inverse = inverse.to_owned();
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            correct_inverse_method(corrector, node, method_call, method, &inverse);
        });
    }

    fn on_block<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = INVERSE_BLOCK.match_node(node) else {
            return;
        };
        let method_call = captures[0].node().unwrap();
        let Some(method) = captures[1].sym() else {
            return;
        };
        let Some(block) = captures[2].node() else {
            return;
        };
        let inverse_blocks = inverse_map(cx, "InverseBlocks");
        let Some(inverse) = lookup(&inverse_blocks, method) else {
            return;
        };
        if negated(node) && node.parent().is_some_and(negated) {
            return;
        }
        if node.each_node().any(|n| n.is(NodeType::Next)) {
            return;
        }
        // Inverse method offenses inside of the block of an inverse method
        // offense, such as `y.reject { |key, _value| !(key =~ /c\d/) }`, can
        // cause autocorrection to apply improper corrections.
        self.ignored.ignore(block);
        let message = format!("Use `{inverse}` instead of inverting `{method}`.");
        let inverse = inverse.to_owned();
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(method_call.loc().selector.unwrap(), &inverse);
            correct_inverse_selector(block, corrector);
        });
    }
}

fn correct_inverse_method(
    corrector: &mut Corrector,
    node: Node<'_>,
    method_call: Node<'_>,
    method: &str,
    inverse: &str,
) {
    corrector.remove(not_to_receiver(node, method_call));
    corrector.replace(method_call.loc().selector.unwrap(), inverse);
    remove_end_parenthesis(corrector, node, method, method_call);
}

fn correct_inverse_selector(block: Node<'_>, corrector: &mut Corrector) {
    let selector_loc = block.loc().selector.unwrap();
    let selector = block.range_source(selector_loc);
    if NEGATED_EQUALITY_METHODS.contains(&selector) {
        let replaced = format!("={}", &selector[1..]);
        corrector.replace(selector_loc, replaced);
    } else {
        if block.loc().dot.is_some() {
            corrector.remove(dot_range(block));
        }
        corrector.remove(selector_loc);
    }
}

fn negated(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is_method("!"))
}

fn not_to_receiver(node: Node<'_>, method_call: Node<'_>) -> Range {
    node.loc()
        .selector
        .unwrap()
        .begin()
        .join(method_call.source_range().begin())
}

fn end_parentheses(node: Node<'_>, method_call: Node<'_>) -> Range {
    method_call
        .source_range()
        .end()
        .join(node.source_range().end())
}

fn safe_navigation_incompatible(node: Node<'_>) -> bool {
    node.is(NodeType::Csend)
        && node
            .method_name()
            .is_some_and(|m| SAFE_NAVIGATION_INCOMPATIBLE_METHODS.contains(&m))
}

/// When comparing classes, `!(Integer < Numeric)` is not the same as
/// `Integer > Numeric`.
fn possible_class_hierarchy_check(lhs: Node<'_>, rhs: &[Node<'_>], method: &str) -> bool {
    CLASS_COMPARISON_METHODS.contains(&method)
        && (camel_case_constant(lhs) || (rhs.len() == 1 && camel_case_constant(rhs[0])))
}

fn camel_case_constant(node: Node<'_>) -> bool {
    node.is(NodeType::Const) && regex!(r"[A-Z]+[a-z]+").find(node.source()).is_some()
}

fn dot_range(node: Node<'_>) -> Range {
    range_between(
        node.loc().dot.unwrap().begin_pos,
        node.source_range().end_pos,
    )
}

fn remove_end_parenthesis(corrector: &mut Corrector, node: Node<'_>, method: &str, method_call: Node<'_>) {
    if !(EQUALITY_METHODS.contains(&method)
        || method_call.parent().is_some_and(|p| p.is(NodeType::Begin)))
    {
        return;
    }
    corrector.remove(end_parentheses(node, method_call));
}
