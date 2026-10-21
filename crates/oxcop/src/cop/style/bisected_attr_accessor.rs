//! Port of `lib/rubocop/cop/style/bisected_attr_accessor.rb` and its
//! `bisected_attr_accessor/macro.rb`.

use std::collections::HashMap;

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::mixin::util::indent;
use crate::cop::mixin::visibility_help::node_visibility;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct BisectedAttrAccessor {
    /// `@macros_to_rewrite`: per class, the macros to rewrite (with their
    /// bisected attributes), in order.
    macros_to_rewrite: HashMap<NodeId, Vec<(NodeId, Vec<NodeId>)>>,
}

impl Cop for BisectedAttrAccessor {
    fn name(&self) -> &'static str {
        "Style/BisectedAttrAccessor"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass, NodeType::Module]
    }

    fn after_node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass, NodeType::Module]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.macros_to_rewrite.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, class_node: Node<'a>) {
        let mut to_rewrite = Vec::new();
        for macros in find_macros(class_node.body()) {
            let bisected = find_bisection(&macros);
            if bisected.is_empty() {
                continue;
            }
            for macro_ in &macros {
                let attrs = macro_.bisect(&bisected);
                if attrs.is_empty() {
                    continue;
                }
                to_rewrite.push((macro_.node.id(), attrs.iter().map(|a| a.id()).collect()));
                for attr in attrs {
                    let message = format!(
                        "Combine both accessors into `attr_accessor {}`.",
                        attr.source()
                    );
                    cx.add_offense(attr.source_range(), &message);
                }
            }
        }
        self.macros_to_rewrite.insert(class_node.id(), to_rewrite);
    }

    fn after_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, class_node: Node<'a>) {
        let Some(macros) = self.macros_to_rewrite.get(&class_node.id()) else {
            return;
        };
        let ast = class_node.ast();
        let ps = cx.processed_source;
        for (node_id, bisection) in macros {
            let node = ast.node(*node_id);
            let macro_ = Macro::new(node);
            let bisected_names: Vec<&str> =
                bisection.iter().map(|id| ast.node(*id).source()).collect();
            let rest: Vec<&str> = macro_
                .attr_names()
                .into_iter()
                .filter(|n| !bisected_names.contains(n))
                .collect();
            let range = range_by_whole_lines(ps, node.source_range(), true);
            cx.correct(|corrector| {
                if macro_.is_writer() {
                    if rest.is_empty() {
                        corrector.remove(range);
                    } else {
                        corrector.replace(
                            node.source_range(),
                            format!("attr_writer {}", rest.join(", ")),
                        );
                    }
                } else {
                    let attr_accessor = format!("attr_accessor {}\n", bisected_names.join(", "));
                    if rest.is_empty() {
                        corrector.replace(range, format!("{}{attr_accessor}", indent(node, 0)));
                    } else {
                        corrector.insert_before(node.source_range(), attr_accessor);
                        corrector.replace(
                            node.source_range(),
                            format!("{}attr_reader {}", indent(node, 0), rest.join(", ")),
                        );
                    }
                }
            });
        }
    }
}

struct Macro<'a> {
    node: Node<'a>,
    /// `attrs`: source => node (later duplicates win, first position kept).
    attrs: Vec<(&'a str, Node<'a>)>,
}

impl<'a> Macro<'a> {
    fn is_macro(node: Node<'_>) -> bool {
        node.is_method("attr_reader") || node.is_method("attr_writer") || node.is_method("attr")
    }

    fn new(node: Node<'a>) -> Self {
        let mut attrs: Vec<(&str, Node<'_>)> = Vec::new();
        for attr in node.arguments() {
            match attrs.iter_mut().find(|(source, _)| *source == attr.source()) {
                Some(entry) => entry.1 = attr,
                None => attrs.push((attr.source(), attr)),
            }
        }
        Self { node, attrs }
    }

    /// `bisect(*names)`: the attributes named, in the order of `names`.
    fn bisect(&self, names: &[&str]) -> Vec<Node<'a>> {
        names
            .iter()
            .filter_map(|name| self.attrs.iter().find(|(s, _)| s == name).map(|(_, n)| *n))
            .collect()
    }

    fn attr_names(&self) -> Vec<&'a str> {
        self.attrs.iter().map(|(s, _)| *s).collect()
    }

    fn is_reader(&self) -> bool {
        self.node.is_method("attr_reader") || self.node.is_method("attr")
    }

    fn is_writer(&self) -> bool {
        self.node.is_method("attr_writer")
    }
}

/// `find_macros(class_def)`: the macros grouped by visibility.
fn find_macros(class_def: Option<Node<'_>>) -> Vec<Vec<Macro<'_>>> {
    let Some(class_def) = class_def else {
        return Vec::new();
    };
    if class_def.is(NodeType::Def) {
        return Vec::new();
    }
    let send_nodes: Vec<Node<'_>> = if class_def.is_send() {
        vec![class_def]
    } else {
        class_def.child_nodes().filter(|c| c.is_send()).collect()
    };
    let mut groups: Vec<(&str, Vec<Macro<'_>>)> = Vec::new();
    for node in send_nodes {
        if !Macro::is_macro(node) {
            continue;
        }
        let visibility = node_visibility(node);
        let macro_ = Macro::new(node);
        match groups.iter_mut().find(|(v, _)| *v == visibility) {
            Some((_, group)) => group.push(macro_),
            None => groups.push((visibility, vec![macro_])),
        }
    }
    groups.into_iter().map(|(_, g)| g).collect()
}

/// `find_bisection(macros)`: names both read and written.
fn find_bisection<'a>(macros: &[Macro<'a>]) -> Vec<&'a str> {
    let readers: Vec<&str> = macros
        .iter()
        .filter(|m| m.is_reader())
        .flat_map(|m| m.attr_names())
        .collect();
    let writers: Vec<&str> = macros
        .iter()
        .filter(|m| !m.is_reader())
        .flat_map(|m| m.attr_names())
        .collect();
    let mut out: Vec<&str> = Vec::new();
    for name in readers {
        if writers.contains(&name) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}
