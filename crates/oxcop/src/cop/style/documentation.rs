//! Port of `lib/rubocop/cop/style/documentation.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::documentation_comment::documentation_comment;
use crate::cop::mixin::project_index_help::{FILE_URI_PREFIX, resolve_constant_in_index, same_file};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;
use crate::regex;

static CONSTANT_DEFINITION: NodePattern = NodePattern::new("{class module casgn}");
static OUTER_MODULE: NodePattern = NodePattern::new("(const (const nil? _) _)");
static CONSTANT_VISIBILITY_DECLARATION: NodePattern =
    NodePattern::new("(send nil? {:public_constant :private_constant} ({sym str} _))");
static INCLUDE_STATEMENT: NodePattern =
    NodePattern::new("(send nil? {:include :extend :prepend} const)");

#[derive(Default)]
pub struct Documentation;

impl Cop for Documentation {
    fn name(&self) -> &'static str {
        "Style/Documentation"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = node.body();
        if node.is(NodeType::Class) && body.is_none() {
            return;
        }
        check(cx, node, body);
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>, body: Option<Node<'_>>) {
    if namespace(body) || documentation_comment(cx, node) || constant_allowed(cx, node) {
        return;
    }
    if nodoc_self_or_outer_module(cx.processed_source, node) {
        return;
    }
    if body.is_some_and(|b| include_statement_only(&Child::Node(b.id()), b)) {
        return;
    }
    if documented_elsewhere(cx, node) {
        return;
    }
    let range = range_between(
        node.source_range().begin_pos,
        node.loc().name.unwrap().end_pos,
    );
    let message = format!(
        "Missing top-level documentation comment for `{} {}`.",
        node.ty().name(),
        identifier(node)
    );
    cx.add_offense(range, &message);
}

fn nodoc_self_or_outer_module(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    nodoc_comment(ps, Some(node), false)
        || (compact_namespace(node)
            && nodoc_comment(
                ps,
                OUTER_MODULE.search(node).first().map(|(n, _)| *n),
                false,
            ))
}

/// `include_statement_only?(body)` for any child value (`respond_to?(:children)`
/// is false for symbols and `nil`).
fn include_statement_only(child: &Child, any: Node<'_>) -> bool {
    let Child::Node(id) = child else {
        return false;
    };
    let node = any.ast().node(*id);
    INCLUDE_STATEMENT.is_match(node)
        || node
            .children()
            .iter()
            .all(|c| include_statement_only(c, node))
}

fn namespace(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if node.is(NodeType::Begin) {
        node.child_nodes().all(|child| {
            CONSTANT_DEFINITION.is_match(child) || CONSTANT_VISIBILITY_DECLARATION.is_match(child)
        }) && node.children().iter().all(|c| matches!(c, Child::Node(_)))
    } else {
        CONSTANT_DEFINITION.is_match(node)
    }
}

fn constant_allowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let short_name = node.identifier().and_then(|i| i.short_name()).unwrap_or("");
    cx.cop_config_list("AllowedConstants").contains(&short_name)
}

fn compact_namespace(node: Node<'_>) -> bool {
    node.range_source(node.loc().name.unwrap()).contains("::")
}

fn nodoc_comment(ps: &ProcessedSource<'_>, node: Option<Node<'_>>, require_all: bool) -> bool {
    let Some(node) = node else {
        return false;
    };
    let Some(first) = node.child(0) else {
        return false;
    };
    if let Some(nodoc) = ps.node_comments(first).first()
        && ps.line_of(nodoc.range.begin_pos) == node.first_line()
        && is_nodoc(nodoc.text, require_all)
    {
        return true;
    }
    nodoc_comment(ps, node.parent(), true)
}

fn is_nodoc(text: &str, require_all: bool) -> bool {
    // `"\s+all\s*$"` is in a double-quoted string, where `\s` is a space.
    if require_all {
        regex!(r"^#\s*:nodoc: +all *$").find(text).is_some()
    } else {
        regex!(r"^#\s*:nodoc:").find(text).is_some()
    }
}

fn identifier(node: Node<'_>) -> String {
    let mut nodes = vec![node];
    nodes.extend(
        node.ancestors()
            .filter(|a| matches!(a.ty(), NodeType::Class | NodeType::Module)),
    );
    let mut parts: Vec<String> = Vec::new();
    for n in nodes.iter().rev() {
        qualify_const(n.identifier(), &mut parts);
    }
    parts.join("::").replacen("::::", "::", 1)
}

fn qualify_const(node: Option<Node<'_>>, out: &mut Vec<String>) {
    let Some(node) = node else {
        return;
    };
    if matches!(node.ty(), NodeType::Cbase | NodeType::SelfNode)
        || node.is_call()
        || node.is_variable()
    {
        out.push(node.source().to_owned());
    } else {
        qualify_const(node.namespace(), out);
        if let Some(short_name) = node.short_name() {
            out.push(short_name.to_owned());
        }
    }
}

/// `documented_elsewhere?`: with the project index, the documentation
/// requirement is satisfied by a comment on any other definition of the
/// same class or module.
fn documented_elsewhere(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(index) = cx.project_index() else {
        return false;
    };
    let Some(declaration) = node
        .identifier()
        .and_then(|identifier| resolve_constant_in_index(&index, identifier))
        .filter(|declaration| declaration.is_namespace())
    else {
        return false;
    };
    let current = cx.processed_source.file_path();
    index.definitions(declaration).into_iter().any(|definition| {
        let Some(location) = index.location(definition) else {
            return false;
        };
        // `other_definition?`
        let other = location.uri.starts_with(FILE_URI_PREFIX)
            && location.file_path.is_some_and(|path| {
                !same_file(&path, std::path::Path::new(&current))
                    || location.start_line as usize + 1 != node.first_line()
            });
        // `documenting_comments?`
        other
            && index.comments(definition).iter().any(|comment| {
                let text = comment.trim();
                !text.is_empty()
                    && !regex!(
                        r"\A\#\s*(rubocop:|frozen_string_literal:|encoding:|shareable_constant_value:|typed:|-\*-)"
                    )
                    .find(text)
                    .is_some()
            })
    })
}
