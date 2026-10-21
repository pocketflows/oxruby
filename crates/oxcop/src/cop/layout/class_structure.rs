//! Port of `lib/rubocop/cop/layout/class_structure.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::visibility_help::{node_visibility, visibility_block};
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

const VISIBILITY_SCOPES: &[&str] = &["private", "protected", "public"];

#[derive(Default)]
pub struct ClassStructure;

impl Cop for ClassStructure {
    fn name(&self) -> &'static str {
        "Layout/ClassStructure"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, class_node: Node<'a>) {
        let settings = Settings::new(cx);
        let ps = cx.processed_source;
        for (node, category, previous) in settings.out_of_order_elements(class_node).into_iter().rev()
        {
            let message = format!("`{category}` is supposed to appear before `{previous}`.");
            let settings = &settings;
            cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
                settings.autocorrect(ps, corrector, node);
            });
        }
    }
}

/// `expected_order` and `categories`.
struct Settings<'c> {
    expected_order: Vec<&'c str>,
    categories: Vec<(&'c str, Vec<&'c str>)>,
}

impl<'c> Settings<'c> {
    fn new(cx: &Investigation<'c, '_>) -> Self {
        let expected_order = cx.cop_config_list("ExpectedOrder");
        let categories = match cx.cop_config_value("Categories") {
            Some(Value::Mapping(map)) => map
                .iter()
                .filter_map(|(k, v)| {
                    let names = match v {
                        Value::Sequence(items) => items.iter().filter_map(Value::as_str).collect(),
                        _ => Vec::new(),
                    };
                    Some((k.as_str()?, names))
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            expected_order,
            categories,
        }
    }

    fn index(&self, category: &str) -> Option<usize> {
        self.expected_order.iter().position(|c| *c == category)
    }

    fn out_of_order_elements<'a>(&self, class_node: Node<'a>) -> Vec<(Node<'a>, String, String)> {
        let mut out_of_order = Vec::new();
        let mut max_index: isize = -1;
        let mut previous_category: Option<String> = None;
        for node in class_elements(class_node) {
            let category = self.classify(node);
            if self.ignore(node, &category) {
                continue;
            }
            let index = self.index(&category).unwrap() as isize;
            if index < max_index && previous_category.as_deref() != Some(category.as_str()) {
                out_of_order.push((
                    node,
                    category.clone(),
                    self.expected_order[max_index as usize].to_owned(),
                ));
            }
            if index > max_index {
                max_index = index;
            }
            previous_category = Some(category);
        }
        out_of_order
    }

    fn autocorrect(&self, ps: &ProcessedSource<'_>, corrector: &mut crate::cop::Corrector, node: Node<'_>) {
        if dynamic_constant(node) {
            return;
        }
        let Some(anchor) = self.insertion_anchor(node) else {
            return;
        };
        let anchor_range = source_range_with_comment(ps, anchor);
        for group_node in self.movable_group(node).into_iter().rev() {
            let current_range = source_range_with_comment(ps, group_node);
            corrector.insert_before(anchor_range, ps.slice(current_range));
            corrector.remove(current_range);
        }
    }

    fn classify(&self, node: Node<'_>) -> String {
        match node.ty() {
            NodeType::Block => match node.send_node() {
                Some(send) => self.classify(send),
                None => String::new(),
            },
            NodeType::Send => self.find_send_node_category(node),
            _ => {
                let name = humanize_node(node);
                self.find_category(&name)
                    .map(str::to_owned)
                    .unwrap_or(name)
            }
        }
    }

    fn find_send_node_category(&self, node: Node<'_>) -> String {
        let name = node.method_name().unwrap_or("");
        let key = self.find_category(name).unwrap_or(name);
        let visibility_key = if node.is_def_modifier() {
            if name.ends_with("_class_method") {
                format!("{name}s")
            } else {
                format!("{name}_methods")
            }
        } else {
            format!("{}_{key}", node_visibility(node))
        };
        if self.expected_order.contains(&visibility_key.as_str()) {
            visibility_key
        } else {
            key.to_owned()
        }
    }

    fn find_category(&self, name: &str) -> Option<&'c str> {
        self.categories
            .iter()
            .find(|(_, names)| names.contains(&name))
            .map(|(category, _)| *category)
    }

    fn ignore(&self, node: Node<'_>, classification: &str) -> bool {
        classification.ends_with('=') || self.index(classification).is_none() || private_constant(node)
    }

    fn insertion_anchor<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let index = self.index(&self.classify(node))?;
        self.movable_span(node).into_iter().find(|sibling| {
            let classification = self.classify(*sibling);
            !self.ignore(*sibling, &classification)
                && self.index(&classification).is_some_and(|i| i > index)
        })
    }

    fn movable_group<'a>(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let classification = self.classify(node);
        let mut group = vec![node];
        for sibling in node.right_siblings() {
            if self.classify(sibling) != classification {
                break;
            }
            if self.ignore(sibling, &classification) || dynamic_constant(sibling) {
                break;
            }
            group.push(sibling);
        }
        group
    }

    fn movable_span<'a>(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let left_siblings = node.left_siblings();
        match left_siblings
            .iter()
            .rposition(|sibling| self.barrier(node, *sibling))
        {
            Some(barrier_index) => left_siblings[barrier_index + 1..].to_vec(),
            None => left_siblings,
        }
    }

    fn barrier(&self, node: Node<'_>, sibling: Node<'_>) -> bool {
        dynamic_constant(sibling) || (self.visibility_dependent(node) && visibility_block(sibling))
    }

    fn visibility_dependent(&self, node: Node<'_>) -> bool {
        if node.is(NodeType::Def) {
            return true;
        }
        if !node.is_send() || node.is_def_modifier() {
            return false;
        }
        let method_name = node.method_name().unwrap_or("");
        let key = self.find_category(method_name).unwrap_or(method_name);
        VISIBILITY_SCOPES
            .iter()
            .any(|visibility| self.expected_order.contains(&format!("{visibility}_{key}").as_str()))
    }
}

fn class_elements(class_node: Node<'_>) -> Vec<Node<'_>> {
    let Some(class_def) = class_node.body() else {
        return Vec::new();
    };
    if class_def.is_any(&[NodeType::Begin, NodeType::Kwbegin]) {
        flatten_class_elements(class_def)
    } else {
        vec![class_def]
    }
}

fn flatten_class_elements(node: Node<'_>) -> Vec<Node<'_>> {
    node.child_nodes()
        .flat_map(|child| {
            if child.is(NodeType::Kwbegin) {
                flatten_class_elements(child)
            } else {
                vec![child]
            }
        })
        .collect()
}

fn humanize_node(node: Node<'_>) -> String {
    if node.is(NodeType::Def) {
        if node.is_method("initialize") {
            return "initializer".to_owned();
        }
        return format!("{}_methods", node_visibility(node));
    }
    match node.ty() {
        NodeType::Casgn => "constants",
        NodeType::Defs => "public_class_methods",
        NodeType::Def => "public_methods",
        NodeType::Sclass => "class_singleton",
        other => other.name(),
    }
    .to_owned()
}

fn dynamic_constant(node: Node<'_>) -> bool {
    if !(node.is(NodeType::Casgn) && node.namespace().is_none()) {
        return false;
    }
    let Some(expression) = node.expression() else {
        return false;
    };
    expression.is_send()
        && !(expression.is_method("freeze")
            && expression
                .receiver()
                .is_some_and(|r| r.is_recursive_basic_literal()))
}

fn private_constant(node: Node<'_>) -> bool {
    if !(node.is(NodeType::Casgn) && node.namespace().is_none()) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let name = node.name();
    parent
        .child_nodes()
        .filter(|c| c.is(NodeType::Send))
        .any(|child| marked_as_private_constant(child, name))
}

fn marked_as_private_constant(node: Node<'_>, name: Option<&str>) -> bool {
    if !node.is_method("private_constant") {
        return false;
    }
    // `arg.value == name`: only a symbol equals the constant's (Symbol) name.
    node.arguments()
        .iter()
        .any(|arg| arg.is(NodeType::Sym) && arg.sym_value().is_some() && arg.sym_value() == name)
}

/// `source_range_with_comment`, with this cop's `begin_pos_with_comment`
/// and `end_position_for`.
fn source_range_with_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    Range::new(begin_pos_with_comment(ps, node), end_position_for(ps, node))
}

fn end_position_for(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    if node.is(NodeType::Casgn)
        && let Some(heredoc) = node.each_node().find(|n| n.is_any_str() && n.is_heredoc())
    {
        return heredoc.loc().heredoc_end.unwrap().end_pos + 1;
    }
    let end_line = ps.line_of(node.source_range().end_pos);
    ps.line_range(end_line).end_pos
}

fn begin_pos_with_comment(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let mut first_comment_line = None;
    for annotation_line in (1..node.first_line()).rev() {
        if ps.comment_at_line(annotation_line).is_none() {
            break;
        }
        if whole_line_comment_at_line(ps, annotation_line) {
            first_comment_line = Some(annotation_line);
        }
    }
    let line = first_comment_line.unwrap_or(node.first_line());
    ps.line_start(line).saturating_sub(1)
}

fn whole_line_comment_at_line(ps: &ProcessedSource<'_>, line: usize) -> bool {
    regex!(r"\A\s*#").find(ps.lines()[line - 1]).is_some()
}
