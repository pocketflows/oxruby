//! Port of `lib/rubocop/cop/style/redundant_regexp_escape.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{char_offset, contents_range};
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::processed_source::Range;
use crate::regexp_parser::{Event, Tree};

const MSG_REDUNDANT_ESCAPE: &str = "Redundant escape inside regexp literal";
const ALLOWED_ALWAYS_ESCAPES: &[char] = &[' ', '\n', '[', ']', '^', '\\', '#'];
const ALLOWED_WITHIN_CHAR_CLASS_METACHAR_ESCAPES: &[char] = &['-'];
const ALLOWED_OUTSIDE_CHAR_CLASS_METACHAR_ESCAPES: &[char] =
    &['.', '*', '+', '?', '{', '}', '(', ')', '|', '$'];

#[derive(Default)]
pub struct RedundantRegexpEscape;

impl Cop for RedundantRegexpEscape {
    fn name(&self) -> &'static str {
        "Style/RedundantRegexpEscape"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for (c, index, within_character_class) in each_escape(node) {
            if allowed_escape(node, c, index, within_character_class) {
                continue;
            }
            let location = escape_range_at_index(node, index);
            cx.add_offense_with_correction(location, MSG_REDUNDANT_ESCAPE, |corrector| {
                corrector.remove_leading(location, 1);
            });
        }
    }
}

fn contents(node: Node<'_>) -> Vec<char> {
    node.range_source(contents_range(node)).chars().collect()
}

/// `content[i]`, with Ruby's negative indexing.
fn char_at(content: &[char], i: isize) -> Option<char> {
    let i = if i < 0 { content.len() as isize + i } else { i };
    (i >= 0).then(|| content.get(i as usize).copied()).flatten()
}

fn allowed_escape(node: Node<'_>, c: char, index: usize, within_character_class: bool) -> bool {
    if c.is_alphanumeric() {
        return true;
    }
    if ALLOWED_ALWAYS_ESCAPES.contains(&c) || delimiter(node, c) {
        return true;
    }
    if requires_escape_to_avoid_interpolation(node, index, c) {
        return true;
    }
    if within_character_class {
        ALLOWED_WITHIN_CHAR_CLASS_METACHAR_ESCAPES.contains(&c)
            && !char_class_begins_or_ends_with_escaped_hyphen(node, index)
    } else {
        ALLOWED_OUTSIDE_CHAR_CLASS_METACHAR_ESCAPES.contains(&c)
    }
}

fn char_class_begins_or_ends_with_escaped_hyphen(node: Node<'_>, index: usize) -> bool {
    let content = contents(node);
    let index = index as isize;
    if char_at(&content, index + 2) == Some(']') {
        true
    } else if char_at(&content, index - 1) == Some('[') {
        index < 2 || char_at(&content, index - 2) != Some('\\')
    } else {
        false
    }
}

fn delimiter(node: Node<'_>, c: char) -> bool {
    let begin = node.range_source(node.loc().begin.unwrap());
    let end = node.range_source(node.loc().end.unwrap());
    begin.ends_with(c) || end.starts_with(c)
}

fn requires_escape_to_avoid_interpolation(node: Node<'_>, index: usize, escaped: char) -> bool {
    if index == 0 || !matches!(escaped, '@' | '$') {
        return false;
    }
    char_at(&contents(node), index as isize - 1) == Some('#')
}

/// `each_escape`: `(char, index, within_character_class)` of each escape.
fn each_escape(node: Node<'_>) -> Vec<(char, usize, bool)> {
    let Some(tree) = parsed_tree(node) else {
        return Vec::new();
    };
    let tree = &tree.tree;
    let mut depth = 0i64;
    let mut out = Vec::new();
    for (event, expr) in tree.traverse(Tree::ROOT, false) {
        let exp = tree.get(expr);
        if exp.is_type("escape")
            && let Some(c) = exp.text.chars().nth(1)
        {
            out.push((c, tree.ts(expr), depth != 0));
        }
        if exp.is_type("set") {
            depth += if event == Event::Enter { 1 } else { -1 };
        }
    }
    out
}

fn escape_range_at_index(node: Node<'_>, index: usize) -> Range {
    let src = node.ast().source();
    let regexp_begin = node.loc().begin.unwrap().end_pos;
    let start = char_offset(src, regexp_begin, index);
    Range::new(start, char_offset(src, start, 2))
}
