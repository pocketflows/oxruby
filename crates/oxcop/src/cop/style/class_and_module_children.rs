//! Port of `lib/rubocop/cop/style/class_and_module_children.rb`.
//!
//! The `rubydex` project index isn't supported, so the namespace keyword
//! always comes from `heuristic_namespace_keyword` and every namespace is
//! compactible, as upstream without an index.

use crate::ast::{Child, Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{configured_indentation_width, indentation};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::ProcessedSource;

const NESTED_MSG: &str = "Use nested module/class definitions instead of compact style.";
const COMPACT_MSG: &str = "Use compact module/class definition instead of nested style.";

#[derive(Clone, Copy)]
struct Styles<'a> {
    classes: &'a str,
    modules: &'a str,
    indentation_width: usize,
}

impl Styles<'_> {
    fn for_kind(&self, kind: NodeType) -> &str {
        if kind == NodeType::Class {
            self.classes
        } else {
            self.modules
        }
    }
}

#[derive(Default)]
pub struct ClassAndModuleChildren;

impl Cop for ClassAndModuleChildren {
    fn name(&self) -> &'static str {
        "Style/ClassAndModuleChildren"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let styles = Styles {
            classes: cx
                .cop_config_str("EnforcedStyleForClasses")
                .unwrap_or_else(|| style(cx)),
            modules: cx
                .cop_config_str("EnforcedStyleForModules")
                .unwrap_or_else(|| style(cx)),
            indentation_width: configured_indentation_width(cx),
        };
        if node.is(NodeType::Class) && node.parent_class().is_some() && styles.classes != "nested"
        {
            return;
        }
        let style = styles.for_kind(node.ty());
        let Some(identifier) = node.identifier() else {
            return;
        };
        let namespace = identifier.namespace();
        if namespace.is_some_and(|n| n.is(NodeType::Cbase)) || !const_namespace(namespace) {
            return;
        }
        if node
            .parent()
            .is_some_and(|p| matches!(p.ty(), NodeType::Class | NodeType::Module))
        {
            return;
        }
        let message = if style == "nested" {
            if !identifier.source().contains("::") {
                return;
            }
            NESTED_MSG
        } else {
            if !node
                .body()
                .is_some_and(|b| matches!(b.ty(), NodeType::Module | NodeType::Class))
            {
                return;
            }
            COMPACT_MSG
        };
        let ps = cx.processed_source;
        let indentation = indentation(cx, node);
        let config = cx.config;
        cx.add_offense_with_correction(node.loc().name.unwrap(), message, |corrector| {
            if node.is(NodeType::Class)
                && node.parent_class().is_some()
                && styles.classes != "nested"
            {
                return;
            }
            if styles.for_kind(node.ty()) == "nested" {
                nest_definition(ps, corrector, node, styles, &indentation);
            } else {
                compact_definition(ps, config, corrector, node, styles);
            }
        });
    }
}

fn nest_definition(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    styles: Styles<'_>,
    indentation: &str,
) {
    let keyword = heuristic_namespace_keyword(node);
    let kind = if keyword == "class" {
        NodeType::Class
    } else {
        NodeType::Module
    };
    if styles.for_kind(kind) == "compact" {
        return;
    }
    let padding = format!("{indentation}{}", leading_spaces(ps, node));
    let end = node.loc().end.unwrap();
    let padding_for_trailing_end =
        padding.replacen(&" ".repeat(ps.column_of(end.begin_pos)), "", 1);
    let keyword_range = node.loc().keyword.unwrap();
    corrector.replace(keyword_range, keyword);
    // `split_on_double_colon`
    let children_definition = node.child(0).unwrap();
    let double_colon = children_definition.loc().double_colon.unwrap();
    corrector.replace(
        double_colon,
        format!("\n{padding}{} ", ps.slice(keyword_range)),
    );
    // `add_trailing_end`
    corrector.replace(
        end,
        format!(
            "{padding_for_trailing_end}end\n{}end",
            leading_spaces(ps, node)
        ),
    );
}

fn heuristic_namespace_keyword(node: Node<'_>) -> &'static str {
    let namespace = node.identifier().and_then(|i| i.namespace());
    let class_definition = node.left_sibling().is_some_and(|sibling| {
        sibling.each_node().any(|class_node| {
            class_node.is(NodeType::Class)
                && match (class_node.identifier(), namespace) {
                    (Some(a), Some(b)) => a.same_as(b),
                    (None, None) => true,
                    _ => false,
                }
        })
    });
    if class_definition { "class" } else { "module" }
}

fn compact_definition(
    ps: &ProcessedSource<'_>,
    config: &crate::config::Config,
    corrector: &mut Corrector,
    node: Node<'_>,
    styles: Styles<'_>,
) {
    let body = node.body().unwrap();
    if styles.for_kind(body.ty()) == "nested" {
        return;
    }
    // `compact_node`
    let range = range_between(
        node.loc().keyword.unwrap().begin_pos,
        body.loc().name.unwrap().end_pos,
    );
    let mut replacement = format!(
        "{} {}::{}",
        body.ty().name(),
        node.identifier().unwrap().const_name().unwrap_or_default(),
        body.child(0)
            .and_then(|c| c.const_name())
            .unwrap_or_default()
    );
    let body_comments = ps.node_comments(body);
    if !body_comments.is_empty() {
        let mut parts: Vec<String> = body_comments.iter().map(|c| c.text.to_owned()).collect();
        parts.push(replacement);
        replacement = parts.join("\n");
    }
    corrector.replace(range, replacement);
    remove_end(ps, corrector, body);
    unindent(ps, config, corrector, node, styles);
}

fn remove_end(ps: &ProcessedSource<'_>, corrector: &mut Corrector, body: Node<'_>) {
    let name = body.loc().name.unwrap();
    let end = body.loc().end.unwrap();
    let remove_begin_pos = if ps.line_of(name.begin_pos) == ps.line_of(end.begin_pos) {
        name.end_pos
    } else {
        end.begin_pos - leading_spaces(ps, body).len()
    };
    let adjustment = if ps.buffer_source().as_bytes().get(remove_begin_pos) == Some(&b';') {
        0
    } else {
        1
    };
    corrector.remove(range_between(remove_begin_pos, end.end_pos + adjustment));
}

fn unindent(
    ps: &ProcessedSource<'_>,
    config: &crate::config::Config,
    corrector: &mut Corrector,
    node: Node<'_>,
    styles: Styles<'_>,
) {
    let body = node.body().unwrap();
    let Some(Child::Node(last_id)) = body.children().last() else {
        return;
    };
    let last_child = node.ast().node(*last_id);
    let last_child_leading_spaces = leading_spaces(ps, last_child);
    let width = styles.indentation_width;
    if spaces_size(leading_spaces(ps, node), width) == spaces_size(last_child_leading_spaces, width)
    {
        return;
    }
    let column_delta = width as isize - spaces_size(last_child_leading_spaces, width) as isize;
    if column_delta == 0 {
        return;
    }
    alignment_corrector::correct(
        corrector,
        ps,
        config,
        Some(Target::Node(node)),
        column_delta,
        true,
    );
}

/// `leading_spaces(node)`: the whitespace starting the node's first line.
fn leading_spaces<'s>(ps: &ProcessedSource<'s>, node: Node<'_>) -> &'s str {
    let line = ps.source_line(ps.line_of(node.source_range().begin_pos));
    let end = line
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(line.len());
    &line[..end]
}

fn spaces_size(spaces: &str, width: usize) -> usize {
    spaces
        .chars()
        .map(|c| if c == '\t' { width } else { 1 })
        .sum()
}

fn const_namespace(node: Option<Node<'_>>) -> bool {
    match node {
        None => true,
        Some(n) if n.is(NodeType::Cbase) => true,
        Some(n) if n.is(NodeType::Const) => const_namespace(n.namespace()),
        Some(_) => false,
    }
}
