//! Port of `lib/rubocop/cop/metrics/utils/iterating_block.rb`: used to
//! identify iterating blocks like `.map{}` and `.map(&:...)`.

use crate::ast::{Node, NodeType};

pub const KNOWN_ITERATING_METHODS: &[&str] = &[
    // Enumerable
    "all?", "any?", "chain", "chunk", "chunk_while", "collect", "collect_concat", "count",
    "cycle", "detect", "drop", "drop_while", "each", "each_cons", "each_entry", "each_slice",
    "each_with_index", "each_with_object", "entries", "filter", "filter_map", "find",
    "find_all", "find_index", "flat_map", "grep", "grep_v", "group_by", "inject", "lazy",
    "map", "max", "max_by", "min", "min_by", "minmax", "minmax_by", "none?", "one?",
    "partition", "reduce", "reject", "reverse_each", "select", "slice_after", "slice_before",
    "slice_when", "sort", "sort_by", "sum", "take", "take_while", "tally", "to_h", "uniq",
    "zip",
    // Enumerator
    "with_index", "with_object",
    // Array
    "bsearch", "bsearch_index", "collect!", "combination", "d_permutation", "delete_if",
    "each_index", "keep_if", "map!", "permutation", "product", "reject!", "repeat",
    "repeated_combination", "select!", "sort!",
    // Hash
    "each_key", "each_pair", "each_value", "fetch", "fetch_values", "has_key?", "merge",
    "merge!", "transform_keys", "transform_keys!", "transform_values", "transform_values!",
];

/// Returns the name of the method called with a block if node is a block
/// node, or a block-pass node.
pub fn block_method_name<'a>(node: Node<'a>) -> Option<&'a str> {
    match node.ty() {
        NodeType::Block => node.method_name(),
        NodeType::BlockPass => node.parent().and_then(|p| p.method_name()),
        _ => None,
    }
}

/// Returns true only when name is a known iterating type.
pub fn iterating_method(name: &str) -> bool {
    KNOWN_ITERATING_METHODS.contains(&name)
}

/// `iterating_block?(node)`: `None` if node is neither a block node nor a
/// block-pass node, otherwise whether the call is a known iterating call.
pub fn iterating_block(node: Node<'_>) -> Option<bool> {
    block_method_name(node).map(iterating_method)
}
