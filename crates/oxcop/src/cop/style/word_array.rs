//! Port of `lib/rubocop/cop/style/word_array.rb`.

use std::collections::HashMap;

use crate::ast::{Child, Node, NodeId, NodeType};
use crate::cop::mixin::allowed_pattern::config_regex;
use crate::cop::mixin::array_min_size::ArrayStyleState;
use crate::cop::mixin::array_syntax::bracketed_array_of;
use crate::cop::mixin::percent_array::{
    build_bracketed_array_with_appropriate_whitespace, check_bracketed_array,
    check_percent_array, PercentArray,
};
use crate::cop::mixin::util::{to_string_literal, trim_string_interpolation_escape_character};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct WordArray {
    array_style: ArrayStyleState,
    matrix_of_complex_content_cache: HashMap<NodeId, bool>,
}

impl Cop for WordArray {
    fn name(&self) -> &'static str {
        "Style/WordArray"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.matrix_of_complex_content_cache.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if bracketed_array_of(NodeType::Str, node) {
            let values: Vec<Node<'a>> = node.child_nodes().collect();
            if values.iter().any(|v| v.is_heredoc()) {
                return;
            }
            let regex = word_regex(cx);
            if complex_content(&values, regex.as_ref()) {
                return;
            }
            if self.within_matrix_of_complex_content(cx, node) {
                return;
            }
            check_bracketed_array(self, cx, node, "w");
        } else if node.is_percent_literal_of("string") {
            check_percent_array(self, cx, node);
        }
    }
}

impl PercentArray for WordArray {
    const ARRAY_MSG: &'static str = "Use %<prefer>s for an array of words.";
    fn array_style_state(&mut self) -> &mut ArrayStyleState {
        &mut self.array_style
    }

    const PERCENT_MSG: &'static str = "Use `%w` or `%W` for an array of words.";

    fn invalid_percent_array_contents(&self, _cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
        let values: Vec<Node<'_>> = node.child_nodes().collect();
        complex_content(&values, None)
    }

    fn build_bracketed_array(&self, _cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
        if node.child_nodes().next().is_none() {
            return "[]".to_owned();
        }
        let words: Vec<String> = node
            .child_nodes()
            .map(|word| {
                if word.is(NodeType::Dstr) {
                    let string_literal = to_string_literal(word.source());
                    trim_string_interpolation_escape_character(&string_literal)
                } else {
                    to_string_literal(word.str_content().unwrap_or_default())
                }
            })
            .collect();
        build_bracketed_array_with_appropriate_whitespace(&words, node)
    }
}

impl WordArray {
    fn within_matrix_of_complex_content(&mut self, cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
        let Some(parent) = node.parent() else {
            return false;
        };
        if !parent.is(NodeType::Array) {
            return false;
        }
        if let Some(&cached) = self.matrix_of_complex_content_cache.get(&parent.id()) {
            return cached;
        }
        let result = matrix_of_complex_content(cx, parent);
        self.matrix_of_complex_content_cache.insert(parent.id(), result);
        result
    }
}

fn matrix_of_complex_content(cx: &Investigation<'_, '_>, array: Node<'_>) -> bool {
    let regex = word_regex(cx);
    array.child_nodes().all(|v| v.is(NodeType::Array))
        && array.child_nodes().any(|subarray| {
            let values: Vec<Node<'_>> = subarray.child_nodes().collect();
            complex_content(&values, regex.as_ref())
        })
}

/// `complex_content?(strings, complex_regex:)`; strings that aren't valid
/// UTF-8 are complex.
fn complex_content(strings: &[Node<'_>], complex_regex: Option<&onig::Regex>) -> bool {
    strings.iter().any(|s| {
        if !s.is(NodeType::Str) {
            return false;
        }
        match s.value() {
            Some(Child::Bytes(_)) => true,
            Some(Child::Str(string)) => {
                complex_regex.is_some_and(|re| re.find(string).is_none()) || string.contains(' ')
            }
            _ => false,
        }
    })
}

fn word_regex(cx: &Investigation<'_, '_>) -> Option<onig::Regex> {
    cx.cop_config_value("WordRegex").and_then(config_regex)
}
