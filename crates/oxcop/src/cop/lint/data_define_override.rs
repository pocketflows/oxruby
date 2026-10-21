//! Port of `lib/rubocop/cop/lint/data_define_override.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::{ruby_inspect, symbol_inspect};
use crate::cop::{Cop, Investigation};

const DATA_METHOD_NAMES: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "__id__", "__send__", "class", "clone", "deconstruct",
    "deconstruct_keys", "define_singleton_method", "display", "dup", "enum_for", "eql?",
    "equal?", "extend", "freeze", "frozen?", "hash", "inspect", "instance_eval", "instance_exec",
    "instance_of?", "instance_variable_defined?", "instance_variable_get",
    "instance_variable_set", "instance_variables", "is_a?", "itself", "kind_of?", "members",
    "method", "methods", "nil?", "object_id", "private_methods", "protected_methods",
    "public_method", "public_methods", "public_send", "remove_instance_variable", "respond_to?",
    "send", "singleton_class", "singleton_method", "singleton_methods", "tap", "then", "to_enum",
    "to_h", "to_s", "with", "yield_self",
];

static DATA_DEFINE: NodePattern = NodePattern::new("(send (const {nil? cbase} :Data) :define ...)");

#[derive(Default)]
pub struct DataDefineOverride;

impl Cop for DataDefineOverride {
    fn name(&self) -> &'static str {
        "Lint/DataDefineOverride"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["define"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !DATA_DEFINE.is_match(node) {
            return;
        }
        for arg in node.arguments() {
            let (member_name, inspected) = match arg.ty() {
                NodeType::Sym => {
                    let name = arg.sym_value().unwrap();
                    (name, symbol_inspect(name))
                }
                NodeType::Str => {
                    let Some(name) = arg.str_content() else {
                        continue;
                    };
                    (name, ruby_inspect(name))
                }
                _ => continue,
            };
            if !DATA_METHOD_NAMES.contains(&member_name) {
                continue;
            }
            let message = format!(
                "`{inspected}` member overrides `Data#{member_name}` and it may be unexpected."
            );
            cx.add_offense(arg.source_range(), &message);
        }
    }
}
