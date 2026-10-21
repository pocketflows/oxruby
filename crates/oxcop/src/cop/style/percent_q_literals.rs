//! Port of `lib/rubocop/cop/style/percent_q_literals.rb`.

use crate::ast::{Ast, Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::percent_literal::{percent_type, process};
use crate::cop::{Cop, Investigation};

const LOWER_CASE_Q_MSG: &str = "Do not use `%Q` unless interpolation is needed. Use `%q`.";
const UPPER_CASE_Q_MSG: &str = "Use `%Q` instead of `%q`.";

#[derive(Default)]
pub struct PercentQLiterals;

impl Cop for PercentQLiterals {
    fn name(&self) -> &'static str {
        "Style/PercentQLiterals"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, &["%Q", "%q"]) {
            return;
        }
        let style = style(cx);
        let ty = percent_type(node);
        if (style == "lower_case_q" && ty == "%q") || (style == "upper_case_q" && ty == "%Q") {
            return;
        }
        // Report only if changing the case doesn't change the semantics.
        let corrected = corrected(node.source());
        let result = crate::prism::parse(&corrected, cx.target_ruby_version());
        let path = cx.processed_source.path().display().to_string();
        let ast = if result.errors().next().is_none() {
            Ast::from_prism(&result, &corrected, &path)
        } else {
            Ast::default()
        };
        if !same_children(node, ast.root()) {
            return;
        }
        let message = if style == "lower_case_q" {
            LOWER_CASE_Q_MSG
        } else {
            UPPER_CASE_Q_MSG
        };
        cx.add_offense_with_correction(node.loc().begin.unwrap(), message, |corrector| {
            corrector.replace(node.source_range(), &corrected);
        });
    }
}

/// `node.children == ast&.children`.
fn same_children(node: Node<'_>, other: Option<Node<'_>>) -> bool {
    let Some(other) = other else {
        return false;
    };
    let (a, b) = (node.children(), other.children());
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Child::Node(x), Child::Node(y)) => {
                node.ast().node(*x).same_as(other.ast().node(*y))
            }
            (Child::Node(_), _) | (_, Child::Node(_)) => false,
            (x, y) => x == y,
        })
}

/// `src.sub(src[1], src[1].swapcase)`.
fn corrected(src: &str) -> String {
    let Some(c) = src.chars().nth(1) else {
        return src.to_owned();
    };
    let swapped: String = if c.is_uppercase() {
        c.to_lowercase().collect()
    } else {
        c.to_uppercase().collect()
    };
    src.replacen(c, &swapped, 1)
}
