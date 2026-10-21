//! Port of `lib/rubocop/cop/bundler/gem_version.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::gem_declaration::gem_declaration;
use crate::cop::{Cop, Investigation};
use crate::regex;

const REQUIRED_MSG: &str = "Gem version specification is required.";
const FORBIDDEN_MSG: &str = "Gem version specification is forbidden.";

#[derive(Default)]
pub struct GemVersion;

impl Cop for GemVersion {
    fn name(&self) -> &'static str {
        "Bundler/GemVersion"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["gem"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !gem_declaration(node) || allowed_gem(cx, node) {
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

fn allowed_gem(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let name = node.first_argument().and_then(|a| a.str_content()).unwrap_or("");
    cx.cop_config_list("AllowedGems").contains(&name)
}

/// `(send nil? :gem <(str #version_specification?) ...>)`
fn includes_version_specification(node: Node<'_>) -> bool {
    node.arguments().iter().any(|arg| {
        arg.is(NodeType::Str)
            && arg
                .str_content()
                .is_some_and(|s| regex!(r"^\s*[~<>=]*\s*[0-9.]+").find(s).is_some())
    })
}

/// `(send nil? :gem <(hash <(pair (sym {:branch :ref :tag}) (str _)) ...>) ...>)`
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
