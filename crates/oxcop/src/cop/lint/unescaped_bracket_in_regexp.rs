//! Port of `lib/rubocop/cop/lint/unescaped_bracket_in_regexp.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::parse_regexp;
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::processed_source::Range;
use crate::regexp_parser::Tree;

const MSG: &str = "Regular expression has `]` without escape.";

static REGEXP_CONSTRUCTOR: NodePattern = NodePattern::new(
    "(send
       (const {nil? cbase} :Regexp) {:new :compile}
       $str
       ...
     )",
);

#[derive(Default)]
pub struct UnescapedBracketInRegexp;

impl Cop for UnescapedBracketInRegexp {
    fn name(&self) -> &'static str {
        "Lint/UnescapedBracketInRegexp"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new", "compile"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Regexp) {
            if let Some(tree) = parsed_tree(node) {
                detect_offenses_in_tree(cx, node, &tree.tree);
            }
            return;
        }

        if node.descendants().any(|d| d.is(NodeType::Dstr)) {
            return;
        }
        for (_, captures) in REGEXP_CONSTRUCTOR.search(node) {
            let Some(text) = captures[0].node() else {
                continue;
            };
            if let Some(tree) = parse_regexp(text.str_content().unwrap_or_default()) {
                detect_offenses_in_tree(cx, text, &tree);
            }
        }
    }
}

fn detect_offenses_in_tree(cx: &mut Investigation<'_, '_>, node: Node<'_>, tree: &Tree) {
    let mut skip_class_closer = false;
    for expr in tree.each_expression(Tree::ROOT, false) {
        let exp = tree.get(expr);
        if exp.is_type("set") && exp.expressions.is_empty() {
            skip_class_closer = true;
        } else if exp.is_type("literal") {
            skip_class_closer = detect_offenses(cx, node, tree, expr, skip_class_closer);
        }
    }
}

fn detect_offenses(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    tree: &Tree,
    expr: usize,
    mut skip_class_closer: bool,
) -> bool {
    let exp = tree.get(expr);
    let ts = tree.ts(expr);
    let chars: Vec<char> = exp.text.chars().collect();
    for (pos, &c) in chars.iter().enumerate() {
        // `/(?<!\\)\]/`
        if c != ']' || (pos > 0 && chars[pos - 1] == '\\') {
            continue;
        }
        if skip_class_closer {
            skip_class_closer = false;
            continue;
        }
        if ts + pos == 0 {
            continue;
        }

        let Some(location) = range_at_index(node, ts, pos) else {
            continue;
        };
        cx.add_offense_with_correction(location, MSG, |corrector| {
            corrector.replace(location, "\\]");
        });
    }
    skip_class_closer
}

fn range_at_index(node: Node<'_>, index: usize, offset: usize) -> Option<Range> {
    let begin = node.loc().begin?.end_pos;
    let source = node.ast().source();
    let adjustment = index + offset;
    let mut chars = source[begin..].char_indices().skip(adjustment);
    let (start, c) = chars.next()?;
    Some(Range::new(begin + start, begin + start + c.len_utf8()))
}
