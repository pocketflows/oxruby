//! Port of `lib/rubocop/cop/lint/useless_ruby2_keywords.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static METHOD_DEFINITION: NodePattern = NodePattern::new(
    "{
       (def %1 ...)
       (any_block (send _ :define_method (sym %1)) ...)
     }",
);

#[derive(Default)]
pub struct UselessRuby2Keywords;

impl Cop for UselessRuby2Keywords {
    fn name(&self) -> &'static str {
        "Lint/UselessRuby2Keywords"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["ruby2_keywords"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(first_argument) = node.first_argument() else {
            return;
        };
        if first_argument.is(NodeType::Def) {
            inspect_def(cx, node, first_argument);
        } else if first_argument.is(NodeType::Sym) {
            inspect_sym(cx, node, first_argument);
        }
    }
}

fn message(method_name: &str) -> String {
    format!("`ruby2_keywords` is unnecessary for method `{method_name}`.")
}

fn inspect_def(cx: &mut Investigation<'_, '_>, node: Node<'_>, def_node: Node<'_>) {
    if allowed_arguments(def_node.arguments_node()) {
        return;
    }
    let Some(selector) = node.loc().selector else {
        return;
    };
    cx.add_offense(
        selector,
        &message(def_node.method_name().unwrap_or_default()),
    );
}

fn inspect_sym(cx: &mut Investigation<'_, '_>, node: Node<'_>, sym_node: Node<'_>) {
    if node.parent().is_none() {
        return;
    }
    let Some(method_name) = sym_node.sym_value() else {
        return;
    };
    let Some(definition) = find_method_definition(node, method_name) else {
        return;
    };
    // `BlockNode#arguments` of a numbered or `it` block is empty.
    let arguments = if definition.is(NodeType::Def) || definition.is(NodeType::Block) {
        definition.arguments_node()
    } else {
        None
    };
    if allowed_arguments(arguments) {
        return;
    }

    cx.add_offense(node.source_range(), &message(method_name));
}

fn find_method_definition<'a>(node: Node<'a>, method_name: &str) -> Option<Node<'a>> {
    let params = [Param::Sym(method_name)];
    let ctx = MatchContext {
        params: &params,
        ..Default::default()
    };
    for ancestor in node.ancestors() {
        let found = ancestor
            .child_nodes()
            .filter(|child| child.is(NodeType::Def) || child.is_any_block())
            .find(|child| METHOD_DEFINITION.match_with(*child, &ctx).is_some());
        if found.is_some() {
            return found;
        }
        if matches!(
            ancestor.ty(),
            NodeType::Class | NodeType::Module | NodeType::Sclass
        ) {
            return None;
        }
    }
    None
}

fn allowed_arguments(arguments: Option<Node<'_>>) -> bool {
    let Some(arguments) = arguments else {
        return false;
    };
    if arguments.children().is_empty() {
        return false;
    }
    arguments.child_nodes().any(|c| c.is(NodeType::Restarg))
        && !arguments.child_nodes().any(|c| {
            matches!(
                c.ty(),
                NodeType::Kwarg | NodeType::Kwoptarg | NodeType::Kwrestarg
            )
        })
}
