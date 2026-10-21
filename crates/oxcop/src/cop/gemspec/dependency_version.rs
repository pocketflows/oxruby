//! Port of `lib/rubocop/cop/gemspec/dependency_version.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::gemspec_help::gem_specification;
use crate::cop::{Cop, Investigation};
use crate::regex;

const REQUIRED_MSG: &str = "Dependency version specification is required.";
const FORBIDDEN_MSG: &str = "Dependency version specification is forbidden.";
const ADD_DEPENDENCY_METHODS: &[&str] = &[
    "add_dependency",
    "add_runtime_dependency",
    "add_development_dependency",
];

#[derive(Default)]
pub struct DependencyVersion;

impl Cop for DependencyVersion {
    fn name(&self) -> &'static str {
        "Gemspec/DependencyVersion"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        ADD_DEPENDENCY_METHODS
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !add_dependency_method_declaration(cx, node) || allowed_gem(cx, node) {
            return;
        }
        let style = style(cx);
        let offense = match style {
            "required" => {
                !includes_version_specification(node) && !includes_commit_reference(node)
            }
            "forbidden" => includes_version_specification(node) || includes_commit_reference(node),
            _ => false,
        };
        if offense {
            let message = if style == "required" {
                REQUIRED_MSG
            } else {
                FORBIDDEN_MSG
            };
            cx.add_offense(node.source_range(), message);
        }
    }
}

/// `(send (lvar #match_block_variable_name?) #add_dependency_method? ...)`
fn add_dependency_method_declaration(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(receiver) = node.receiver().filter(|r| r.is(NodeType::Lvar)) else {
        return false;
    };
    let Some(block_variable) = gem_specification(cx.processed_source.ast()) else {
        return false;
    };
    receiver.name() == Some(block_variable)
        && node
            .method_name()
            .is_some_and(|m| ADD_DEPENDENCY_METHODS.contains(&m))
}

fn allowed_gem(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.first_argument()
        .and_then(|a| a.str_content())
        .is_some_and(|name| cx.cop_config_list("AllowedGems").contains(&name))
}

/// `(send _ #add_dependency_method? <(str #version_specification?) ...>)`
fn includes_version_specification(node: Node<'_>) -> bool {
    node.arguments().iter().any(|arg| {
        arg.is(NodeType::Str)
            && arg
                .str_content()
                .is_some_and(|s| regex!(r"^\s*[~<>=]*\s*[0-9.]+").find(s).is_some())
    })
}

/// `(send _ #add_dependency_method? <(hash <(pair (sym {:branch :ref :tag}) (str _)) ...>) ...>)`
fn includes_commit_reference(node: Node<'_>) -> bool {
    node.arguments().iter().any(|arg| {
        arg.is(NodeType::Hash)
            && arg.child_nodes().any(|pair| {
                pair.is(NodeType::Pair)
                    && pair.key().is_some_and(|k| {
                        k.is(NodeType::Sym)
                            && matches!(k.children().first(), Some(Child::Sym(s)) if matches!(s.as_str(), "branch" | "ref" | "tag"))
                    })
                    && pair.pair_value().is_some_and(|v| v.is(NodeType::Str))
            })
    })
}
