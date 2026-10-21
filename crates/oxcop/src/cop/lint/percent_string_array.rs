//! Port of `lib/rubocop/cop/lint/percent_string_array.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::percent_literal::process;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Within `%w`/`%W`, quotes and ',' are unnecessary and may be unwanted in the resulting strings.";

#[derive(Default)]
pub struct PercentStringArray;

impl Cop for PercentStringArray {
    fn name(&self) -> &'static str {
        "Lint/PercentStringArray"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, &["%w", "%W"]) {
            return;
        }
        on_percent_literal(cx, node);
    }
}

fn on_percent_literal<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if !contains_quotes_or_commas(node) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        for value in node.child_nodes() {
            let range = value.source_range();
            let source = value.source();
            if let Some(m) = regex!(r#"['"]?,?$"#).find(source) {
                corrector.remove_trailing(range, m.1 - m.0);
            }
            if regex!(r#"^['"]"#).find(source).is_some() {
                corrector.remove_leading(range, 1);
            }
        }
    });
}

fn contains_quotes_or_commas(node: Node<'_>) -> bool {
    node.child_nodes().any(|value| {
        // `value.children.first.to_s.scrub`: a node child (a `dstr` part)
        // prints as an s-expression, which none of the patterns match.
        let literal = match value.children().first() {
            Some(Child::Str(s)) => s.clone(),
            Some(Child::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => return false,
        };
        if !literal.chars().any(char::is_alphanumeric) {
            return false;
        }
        regex!(r",$").find(&literal).is_some()
            || regex!(r"^'.*'$").find(&literal).is_some()
            || regex!(r#"^".*"$"#).find(&literal).is_some()
    })
}
