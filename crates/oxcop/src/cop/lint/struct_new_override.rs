//! Port of `lib/rubocop/cop/lint/struct_new_override.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};

const STRUCT_METHOD_NAMES: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "[]", "[]=", "__id__", "__send__", "all?", "any?",
    "chain", "chunk", "chunk_while", "class", "clone", "collect", "collect_concat", "compact",
    "count", "cycle", "deconstruct", "deconstruct_keys", "define_singleton_method", "detect",
    "dig", "display", "drop", "drop_while", "dup", "each", "each_cons", "each_entry", "each_pair",
    "each_slice", "each_with_index", "each_with_object", "entries", "enum_for", "eql?", "equal?",
    "extend", "filter", "filter_map", "find", "find_all", "find_index", "first", "flat_map",
    "freeze", "frozen?", "grep", "grep_v", "group_by", "hash", "include?", "inject", "inspect",
    "instance_eval", "instance_exec", "instance_of?", "instance_variable_defined?",
    "instance_variable_get", "instance_variable_set", "instance_variables", "is_a?", "itself",
    "kind_of?", "lazy", "length", "map", "max", "max_by", "member?", "members", "method",
    "methods", "min", "min_by", "minmax", "minmax_by", "nil?", "none?", "object_id", "one?",
    "partition", "private_methods", "protected_methods", "public_method", "public_methods",
    "public_send", "reduce", "reject", "remove_instance_variable", "respond_to?", "reverse_each",
    "select", "send", "singleton_class", "singleton_method", "singleton_methods", "size",
    "slice_after", "slice_before", "slice_when", "sort", "sort_by", "sum", "take", "take_while",
    "tally", "tap", "then", "to_a", "to_enum", "to_h", "to_s", "to_set", "uniq", "values",
    "values_at", "yield_self", "zip",
];

static STRUCT_NEW: NodePattern = NodePattern::new("(send (const {nil? cbase} :Struct) :new ...)");

#[derive(Default)]
pub struct StructNewOverride;

impl Cop for StructNewOverride {
    fn name(&self) -> &'static str {
        "Lint/StructNewOverride"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !STRUCT_NEW.is_match(node) {
            return;
        }

        for (index, arg) in node.arguments().into_iter().enumerate() {
            if index == 0 && arg.is(NodeType::Str) {
                continue;
            }
            let (member_name, inspected) = match arg.ty() {
                NodeType::Sym => {
                    let Some(name) = arg.sym_value() else { continue };
                    (name, sym_inspect(name))
                }
                NodeType::Str => {
                    let Some(name) = arg.str_content() else { continue };
                    (name, ruby_inspect(name))
                }
                _ => continue,
            };
            if !STRUCT_METHOD_NAMES.contains(&member_name) {
                continue;
            }

            let message = format!(
                "`{inspected}` member overrides `Struct#{member_name}` and it may be unexpected."
            );
            cx.add_offense(arg.source_range(), &message);
        }
    }
}

/// `Symbol#inspect` for the member names above.
fn sym_inspect(name: &str) -> String {
    let plain = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '?' || c == '!');
    let operator = matches!(
        name,
        "!" | "!=" | "!~" | "<=>" | "==" | "===" | "[]" | "[]="
    );
    if plain || operator {
        format!(":{name}")
    } else {
        format!(":{}", ruby_inspect(name))
    }
}
