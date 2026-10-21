//! Port of `lib/rubocop/cop/style/perl_backrefs.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct PerlBackrefs;

impl Cop for PerlBackrefs {
    fn name(&self) -> &'static str {
        "Style/PerlBackrefs"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::BackRef, NodeType::Gvar, NodeType::NthRef]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(expression) = preferred_expression_to(node) else {
            return;
        };
        let preferred_expression = format!("{}{expression}", constant_prefix(node));
        let message = format!(
            "Prefer `{preferred_expression}` over `{}`.",
            original_expression_of(node)
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let replacement = if derived_from_braceless_interpolation(node) {
                format!("{{{preferred_expression}}}")
            } else {
                preferred_expression.clone()
            };
            corrector.replace(node.source_range(), replacement);
        });
    }
}

fn derived_from_braceless_interpolation(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(p.ty(), NodeType::Dstr | NodeType::Regexp | NodeType::Xstr)
    })
}

fn original_expression_of(node: Node<'_>) -> String {
    match node.children().first() {
        Some(Child::Int(i)) => format!("${i}"),
        Some(Child::Sym(s)) => s.clone(),
        _ => String::new(),
    }
}

fn preferred_expression_to(node: Node<'_>) -> Option<String> {
    // `$+` / `$LAST_PAREN_MATCH` is deliberately not converted.
    match node.children().first()? {
        Child::Int(i) => Some(format!("Regexp.last_match({i})")),
        Child::Sym(s) => match s.as_str() {
            "$&" | "$MATCH" => Some("Regexp.last_match(0)".to_owned()),
            "$`" | "$PREMATCH" => Some("Regexp.last_match.pre_match".to_owned()),
            "$'" | "$POSTMATCH" => Some("Regexp.last_match.post_match".to_owned()),
            _ => None,
        },
        _ => None,
    }
}

fn constant_prefix(node: Node<'_>) -> &'static str {
    if node
        .ancestors()
        .any(|a| matches!(a.ty(), NodeType::Class | NodeType::Module))
    {
        "::"
    } else {
        ""
    }
}
