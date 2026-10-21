//! Port of `lib/rubocop/cop/lint/ineffective_access_modifier.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const ALTERNATIVE_PRIVATE: &str =
    "`private_class_method` or `private` inside a `class << self` block";
const ALTERNATIVE_PROTECTED: &str = "`protected` inside a `class << self` block";

static PRIVATE_CLASS_METHODS: NodePattern =
    NodePattern::new("(send nil? :private_class_method $...)");

#[derive(Default)]
pub struct IneffectiveAccessModifier;

impl Cop for IneffectiveAccessModifier {
    fn name(&self) -> &'static str {
        "Lint/IneffectiveAccessModifier"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        check_node(cx, node.body());
    }
}

fn check_node<'a>(cx: &mut Investigation<'a, '_>, node: Option<Node<'a>>) {
    let Some(node) = node.filter(|n| n.is(NodeType::Begin)) else {
        return;
    };
    let mut found = Vec::new();
    ineffective_modifier(node, &mut None, None, &mut found);
    for (defs_node, modifier) in found {
        cx.add_offense(defs_node.loc().keyword.unwrap(), &format_message(cx, modifier));
    }
}

/// `private_class_methods(node).to_a.flatten.select(&:basic_literal?).map(&:value)`
fn private_class_method_names(node: Node<'_>) -> Vec<String> {
    PRIVATE_CLASS_METHODS
        .search(node)
        .into_iter()
        .flat_map(|(_, captures)| captures[0].nodes())
        .filter(|n| n.is_basic_literal())
        // Compared with the `defs` method name (a symbol): only symbols match.
        .filter_map(|n| match n.value() {
            Some(Child::Sym(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn format_message(cx: &Investigation<'_, '_>, modifier: Node<'_>) -> String {
    let visibility = modifier.method_name().unwrap();
    let alternative = if visibility == "private" {
        ALTERNATIVE_PRIVATE
    } else {
        ALTERNATIVE_PROTECTED
    };
    let line = cx.processed_source.line_of(modifier.source_range().begin_pos);
    format!(
        "`{visibility}` (on line {line}) does not make singleton methods {visibility}. Use {alternative} instead."
    )
}

fn ineffective_modifier<'a>(
    node: Node<'a>,
    ignored_methods: &mut Option<Vec<String>>,
    mut modifier: Option<Node<'a>>,
    found: &mut Vec<(Node<'a>, Node<'a>)>,
) {
    for child in node.child_nodes() {
        match child.ty() {
            NodeType::Send => {
                if access_modifier(child) {
                    modifier = Some(child);
                }
            }
            NodeType::Defs => {
                let ignored = ignored_methods.get_or_insert_with(|| private_class_method_names(node));
                if correct_visibility(child, modifier, ignored) {
                    continue;
                }
                found.push((child, modifier.unwrap()));
            }
            NodeType::Kwbegin => {
                if ignored_methods.is_none() {
                    *ignored_methods = Some(private_class_method_names(node));
                }
                ineffective_modifier(child, ignored_methods, modifier, found);
            }
            _ => {}
        }
    }
}

fn access_modifier(node: Node<'_>) -> bool {
    node.is_bare_access_modifier() && !node.is_method("module_function")
}

fn correct_visibility(node: Node<'_>, modifier: Option<Node<'_>>, ignored_methods: &[String]) -> bool {
    let Some(modifier) = modifier else {
        return true;
    };
    if modifier.is_method("public") {
        return true;
    }
    node.method_name()
        .is_some_and(|name| ignored_methods.iter().any(|m| m == name))
}
