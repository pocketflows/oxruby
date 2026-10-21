//! Port of `lib/rubocop/cop/mixin/nil_methods.rb`.

use crate::cop::Investigation;
use crate::cop::mixin::allowed_methods::allowed_methods;

/// `nil.methods` in a Ruby process that has loaded RuboCop (Ruby 3.3).
const NIL_CLASS_METHODS: &[&str] = &[
    "!",
    "!=",
    "!~",
    "&",
    "<=>",
    "==",
    "===",
    "=~",
    "^",
    "__id__",
    "__send__",
    "class",
    "clone",
    "define_singleton_method",
    "display",
    "dup",
    "enum_for",
    "eql?",
    "equal?",
    "extend",
    "freeze",
    "frozen?",
    "hash",
    "inspect",
    "instance_eval",
    "instance_exec",
    "instance_of?",
    "instance_variable_defined?",
    "instance_variable_get",
    "instance_variable_set",
    "instance_variables",
    "is_a?",
    "itself",
    "kind_of?",
    "method",
    "methods",
    "nil?",
    "object_id",
    "private_methods",
    "protected_methods",
    "public_method",
    "public_methods",
    "public_send",
    "rationalize",
    "remove_instance_variable",
    "respond_to?",
    "send",
    "singleton_class",
    "singleton_method",
    "singleton_methods",
    "tap",
    "then",
    "to_a",
    "to_c",
    "to_enum",
    "to_f",
    "to_h",
    "to_i",
    "to_json",
    "to_r",
    "to_s",
    "to_yaml",
    "yield_self",
    "|",
];

/// `other_stdlib_methods`.
const OTHER_STDLIB_METHODS: &[&str] = &["to_d"];

/// `nil_methods.include?(name)`.
pub fn nil_method(cx: &Investigation<'_, '_>, name: &str) -> bool {
    NIL_CLASS_METHODS.contains(&name)
        || OTHER_STDLIB_METHODS.contains(&name)
        || allowed_methods(cx).contains(&name)
}
