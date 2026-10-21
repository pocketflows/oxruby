//! Port of `lib/rubocop/cop/style/alias.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Corrector, Cop, Investigation};

const MSG_ALIAS: &str = "Use `alias_method` instead of `alias`.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Lexical,
    Dynamic,
    InstanceEval,
}

#[derive(Default)]
pub struct Alias;

impl Cop for Alias {
    fn name(&self) -> &'static str {
        "Style/Alias"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Alias]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["alias_method"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        if node.is_send() {
            if !node.is_command("alias_method") {
                return;
            }
            if style != "prefer_alias" || !alias_keyword_possible(node) {
                return;
            }
            if node.arguments().len() != 2 || alias_method_value_used(node) {
                return;
            }
            let msg = format!(
                "Use `alias` instead of `alias_method` {}.",
                lexical_scope_type(node)
            );
            cx.add_offense_with_correction(node.loc().selector.unwrap(), &msg, |corrector| {
                autocorrect(corrector, node, style);
            });
            return;
        }
        if !alias_method_possible(node) {
            return;
        }
        if scope_type(node) == Scope::Dynamic || style == "prefer_alias_method" {
            cx.add_offense_with_correction(node.loc().keyword.unwrap(), MSG_ALIAS, |corrector| {
                autocorrect(corrector, node, style);
            });
        } else if !node.child_nodes().any(bareword) {
            let existing: Vec<&str> = node.child_nodes().map(|a| a.source()).collect();
            let preferred: Vec<&str> = node
                .child_nodes()
                .map(|a| a.source().get(1..).unwrap_or(""))
                .collect();
            let msg = format!(
                "Use `alias {}` instead of `alias {}`.",
                preferred.join(" "),
                existing.join(" ")
            );
            let range = node
                .child_nodes()
                .map(|c| c.source_range())
                .reduce(|a, b| a.join(b))
                .unwrap();
            cx.add_offense_with_correction(range, &msg, |corrector| {
                autocorrect(corrector, node, style);
            });
        }
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, style: &str) {
    if node.is_send() {
        let arguments = node.arguments();
        let replacement = format!(
            "alias {} {}",
            identifier(arguments[0]),
            identifier(arguments[1])
        );
        corrector.replace(node.source_range(), replacement);
    } else if scope_type(node) == Scope::Dynamic || style == "prefer_alias_method" {
        let replacement = format!(
            "alias_method {}, {}",
            identifier(node.child(0).unwrap()),
            identifier(node.child(1).unwrap())
        );
        corrector.replace(node.source_range(), replacement);
    } else {
        for identifier in node.child_nodes() {
            corrector.replace(
                identifier.source_range(),
                identifier.source().get(1..).unwrap_or(""),
            );
        }
    }
}

fn alias_keyword_possible(node: Node<'_>) -> bool {
    scope_type(node) != Scope::Dynamic && node.arguments().iter().all(|a| a.is(NodeType::Sym))
}

fn alias_method_value_used(node: Node<'_>) -> bool {
    node.is_argument()
        || node.parent().is_some_and(|p| {
            if p.is_call() {
                p.is_setter_method()
            } else {
                p.is_assignment()
            }
        })
}

fn alias_method_possible(node: Node<'_>) -> bool {
    scope_type(node) != Scope::InstanceEval
        && !node.child_nodes().any(|c| c.is(NodeType::Gvar))
        && !node.ancestors().any(|a| a.is(NodeType::Def))
}

fn scope_type(node: Node<'_>) -> Scope {
    for parent in node.ancestors() {
        match parent.ty() {
            NodeType::Class | NodeType::Module => return Scope::Lexical,
            NodeType::Def | NodeType::Defs => return Scope::Dynamic,
            NodeType::Block | NodeType::Numblock | NodeType::Itblock => {
                if parent.is_method("instance_eval") {
                    return Scope::InstanceEval;
                }
                return Scope::Dynamic;
            }
            _ => {}
        }
    }
    Scope::Lexical
}

fn lexical_scope_type(node: Node<'_>) -> &'static str {
    match node
        .ancestors()
        .find(|a| matches!(a.ty(), NodeType::Class | NodeType::Module))
    {
        None => "at the top level",
        Some(a) if a.is(NodeType::Class) => "in a class body",
        Some(_) => "in a module body",
    }
}

fn bareword(sym_node: Node<'_>) -> bool {
    !sym_node.source().starts_with(':') || sym_node.is(NodeType::Dsym)
}

fn identifier(node: Node<'_>) -> String {
    if node.is(NodeType::Sym) {
        format!(":{}", node.sym_value().unwrap_or(""))
    } else {
        node.source().to_owned()
    }
}
