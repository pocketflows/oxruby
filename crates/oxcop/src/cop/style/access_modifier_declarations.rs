//! Port of `lib/rubocop/cop/style/access_modifier_declarations.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_with_comments_and_lines;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

static ACCESS_MODIFIER_WITH_SYMBOL: NodePattern = NodePattern::new(
    "(send nil? {:private :protected :public :module_function}
       {(sym _)+ (splat {#percent_symbol_array? const send})}
     )",
);

static ACCESS_MODIFIER_WITH_ATTR: NodePattern = NodePattern::new(
    "(send nil? {:private :protected :public :module_function}
       (send nil? {:attr :attr_reader :attr_writer :attr_accessor} _+))",
);

static ACCESS_MODIFIER_WITH_ALIAS_METHOD: NodePattern = NodePattern::new(
    "(send nil? {:private :protected :public :module_function}
       (send nil? :alias_method _ _))",
);

static BARE_ACCESS_MODIFIER_DECLARATION: NodePattern =
    NodePattern::new("(send nil? {:public :protected :private :module_function})");

#[derive(Default)]
pub struct AccessModifierDeclarations;

impl Cop for AccessModifierDeclarations {
    fn name(&self) -> &'static str {
        "Style/AccessModifierDeclarations"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["private", "protected", "public", "module_function"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let group = style(cx) == "group";
        if allowed(cx, node) || !offense(cx, node, group) {
            return;
        }
        let selector = node.loc().selector.unwrap();
        let access_modifier = cx.source(selector);
        let message = if group {
            format!("`{access_modifier}` should not be inlined in method definitions.")
        } else if style(cx) == "inline" {
            format!("`{access_modifier}` should be inlined in method definitions.")
        } else {
            String::new()
        };
        let ps = cx.processed_source;
        let style = style(cx);
        cx.add_offense_with_correction(selector, &message, |corrector| match style {
            "group" => autocorrect_group_style(ps, corrector, node),
            "inline" => autocorrect_inline_style(ps, corrector, node),
            _ => {}
        });
    }
}

fn with_symbol(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "percent_symbol_array?"
            && val
                .node()
                .is_some_and(|n| n.is(NodeType::Array) && n.is_percent_literal_of("symbol"))
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    ACCESS_MODIFIER_WITH_SYMBOL.match_with(node, &ctx).is_some()
}

fn allowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !node.is_access_modifier()
        || node
            .parent()
            .is_some_and(|p| p.is(NodeType::Pair) || p.is_any_block())
        || (cx.cop_config_bool("AllowModifiersOnSymbols", false) && with_symbol(node))
        || (cx.cop_config_bool("AllowModifiersOnAttrs", false)
            && ACCESS_MODIFIER_WITH_ATTR.is_match(node))
        || (cx.cop_config_bool("AllowModifiersOnAliasMethod", false)
            && ACCESS_MODIFIER_WITH_ALIAS_METHOD.is_match(node))
}

fn offense(cx: &Investigation<'_, '_>, node: Node<'_>, group: bool) -> bool {
    if group {
        let skip = match node.parent() {
            Some(parent) => parent.is(NodeType::If),
            None => with_symbol(node),
        };
        if skip {
            return false;
        }
        node.has_arguments() && !right_siblings_same_inline_method(cx, node)
    } else {
        !node.has_arguments() && !select_grouped_def_nodes(node).is_empty()
    }
}

fn correctable_group_offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if style(cx) != "group" || allowed(cx, node) {
        return false;
    }
    node.has_arguments() && !find_corresponding_def_nodes(node).is_empty()
}

fn right_siblings_same_inline_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.right_siblings().iter().any(|sibling| {
        sibling.is_send()
            && correctable_group_offense(cx, *sibling)
            && sibling.method_name() == node.method_name()
            && sibling.has_arguments()
            && !find_corresponding_def_nodes(*sibling).is_empty()
    })
}

fn find_corresponding_def_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    if with_symbol(node) {
        let method_names: Vec<&str> = node
            .arguments()
            .iter()
            .filter_map(|a| a.sym_value())
            .collect();
        let def_nodes: Vec<Node<'_>> = node
            .parent()
            .map(|p| {
                p.child_nodes()
                    .filter(|c| {
                        c.is(NodeType::Def) && method_names.contains(&c.method_name().unwrap_or(""))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if def_nodes.len() == method_names.len() {
            def_nodes
        } else {
            Vec::new()
        }
    } else {
        node.first_argument().into_iter().collect()
    }
}

fn find_argument_less_modifier_node(node: Node<'_>) -> Option<Node<'_>> {
    node.parent()?.child_nodes().find(|child| {
        child.is_send() && child.method_name() == node.method_name() && !child.has_arguments()
    })
}

fn select_grouped_def_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    node.right_siblings()
        .into_iter()
        .take_while(|s| !(s.is_send() && BARE_ACCESS_MODIFIER_DECLARATION.is_match(*s)))
        .filter(|s| s.is(NodeType::Def))
        .collect()
}

fn autocorrect_group_style(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let def_nodes = find_corresponding_def_nodes(node);
    if def_nodes.is_empty() {
        return;
    }
    let source = def_source(ps, node, &def_nodes);
    let method_name = node.method_name().unwrap_or("");
    if let Some(modifier) = find_argument_less_modifier_node(node) {
        corrector.insert_after(modifier.source_range(), format!("\n\n{source}"));
    } else if let Some(ancestor) = node.ancestors().find(|a| {
        matches!(
            a.ty(),
            NodeType::Class | NodeType::Module | NodeType::Sclass
        )
    }) {
        corrector.insert_before(
            ancestor.loc().end.unwrap(),
            format!("{method_name}\n\n{source}\n"),
        );
    } else {
        corrector.replace(node.source_range(), format!("{method_name}\n\n{source}"));
        return;
    }
    for def_node in &def_nodes {
        corrector.remove(range_with_comments_and_lines(ps, *def_node));
    }
    corrector.remove(range_with_comments_and_lines(ps, node));
}

fn autocorrect_inline_style(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    match node.parent().filter(|p| p.is(NodeType::Begin)) {
        Some(_) => {
            if let Some(def_node) = node.right_sibling() {
                let end_pos = first_comment_or_node_start(ps, def_node);
                corrector.remove(node.source_range().begin().join(end_pos));
            }
        }
        None => corrector.remove(range_with_comments_and_lines(ps, node)),
    }
    let modifier_name = node.method_name().unwrap_or("");
    for grouped_def_node in select_grouped_def_nodes(node) {
        corrector.insert_before(grouped_def_node.source_range(), format!("{modifier_name} "));
    }
}

fn first_comment_or_node_start(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let line = node.first_line();
    ps.node_comments(node)
        .iter()
        .find(|c| ps.line_of(c.range.begin_pos) < line)
        .map_or(node.source_range().begin(), |c| c.range.begin())
}

fn def_source(ps: &ProcessedSource<'_>, node: Node<'_>, def_nodes: &[Node<'_>]) -> String {
    let mut parts: Vec<&str> = ps.node_comments(node).iter().map(|c| c.text).collect();
    parts.extend(def_nodes.iter().map(|d| d.source()));
    parts.join("\n")
}
