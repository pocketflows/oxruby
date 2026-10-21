//! Port of `lib/rubocop/cop/style/redundant_regexp_character_class.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::processed_source::Range;
use crate::regex;
use crate::regexp_parser::Tree;

const REQUIRES_ESCAPE_OUTSIDE_CHAR_CLASS_CHARS: &[&str] =
    &[".", "*", "+", "?", "{", "}", "(", ")", "|", "$"];

#[derive(Default)]
pub struct RedundantRegexpCharacterClass;

impl Cop for RedundantRegexpCharacterClass {
    fn name(&self) -> &'static str {
        "Style/RedundantRegexpCharacterClass"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for loc in each_redundant_character_class(node) {
            let source = node.range_source(loc);
            let element = without_character_class(source);
            let message = format!(
                "Redundant single-element character class, `{source}` can be replaced with `{element}`."
            );
            cx.add_offense_with_correction(loc, &message, |corrector| {
                corrector.replace(loc, &element);
            });
        }
    }
}

fn each_redundant_character_class(node: Node<'_>) -> Vec<Range> {
    let Some(parsed) = parsed_tree(node) else {
        return Vec::new();
    };
    let tree = &parsed.tree;
    let mut out = Vec::new();
    for expr in tree.each_expression(Tree::ROOT, false) {
        let exp = tree.get(expr);
        if !exp.is_type("set") || exp.expressions.len() != 1 || exp.negative {
            continue;
        }
        let first = tree.get(exp.expressions[0]);
        if ["set", "posixclass", "nonposixclass"]
            .iter()
            .any(|t| first.is_type(t))
            || multiple_codepoints(&first.text, &first.token)
        {
            continue;
        }
        if !redundant_single_element_character_class(node, &first.text) {
            continue;
        }
        out.push(parsed.body(expr));
    }
    out
}

fn redundant_single_element_character_class(node: Node<'_>, class_elem: &str) -> bool {
    let non_redundant = whitespace_in_free_space_mode(node, class_elem)
        || class_elem == "\\b"
        || regex!(r"\A\\[1-9]\z").find(class_elem).is_some()
        || REQUIRES_ESCAPE_OUTSIDE_CHAR_CLASS_CHARS.contains(&class_elem);
    !non_redundant
}

/// `expression.codepoints.count >= 2` (a `\u{...}` codepoint list).
fn multiple_codepoints(text: &str, token: &str) -> bool {
    token == "codepoint_list" && regex!(r"\h+").find_iter(&text[2..]).count() >= 2
}

fn whitespace_in_free_space_mode(node: Node<'_>, elem: &str) -> bool {
    let extended = node
        .child_nodes()
        .last()
        .is_some_and(|r| r.is(NodeType::Regopt) && r.children().iter().any(|c| c.as_sym() == Some("x")));
    extended && regex!(r"\s").find(elem).is_some()
}

fn without_character_class(source: &str) -> String {
    let mut chars = source.chars();
    chars.next();
    chars.next_back();
    let inner = chars.as_str();
    if source == "[#]" {
        format!("\\{inner}")
    } else {
        inner.to_owned()
    }
}
