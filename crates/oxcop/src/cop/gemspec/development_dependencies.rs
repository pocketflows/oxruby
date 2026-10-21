//! Port of `lib/rubocop/cop/gemspec/development_dependencies.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct DevelopmentDependencies;

impl Cop for DevelopmentDependencies {
    fn name(&self) -> &'static str {
        "Gemspec/DevelopmentDependencies"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["add_development_dependency", "gem"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        let offense = match style.trim_start_matches(':') {
            "Gemfile" | "gems.rb" => add_development_dependency(cx, node),
            "gemspec" => gem(cx, node),
            _ => false,
        };
        if offense {
            let message = format!(
                "Specify development dependencies in {}.",
                style.trim_start_matches(':')
            );
            cx.add_offense(node.source_range(), &message);
        }
    }
}

/// `(send _ :add_development_dependency (str #forbidden_gem? ...) _? _?)`
fn add_development_dependency(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let arguments = node.arguments();
    node.is_method("add_development_dependency")
        && (1..=3).contains(&arguments.len())
        && forbidden_gem_str(cx, arguments[0])
}

/// `(send _ :gem (str #forbidden_gem? ...))`
fn gem(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let arguments = node.arguments();
    node.is_method("gem") && arguments.len() == 1 && forbidden_gem_str(cx, arguments[0])
}

fn forbidden_gem_str(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is(NodeType::Str)
        && node
            .str_content()
            .is_some_and(|name| !cx.cop_config_list("AllowedGems").contains(&name))
}
