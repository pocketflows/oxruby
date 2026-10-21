//! Port of `lib/rubocop/cop/style/hash_conversion.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::util::add_parentheses;
use crate::cop::{Cop, Investigation};

const MSG_TO_H: &str = "Prefer `ary.to_h` to `Hash[ary]`.";
const MSG_LITERAL_MULTI_ARG: &str = "Prefer literal hash to `Hash[arg1, arg2, ...]`.";
const MSG_LITERAL_HASH_ARG: &str = "Prefer literal hash to `Hash[key: value, ...]`.";
const MSG_SPLAT: &str = "Prefer `array_of_pairs.to_h` to `Hash[*array]`.";

static HASH_FROM_ARRAY: NodePattern =
    NodePattern::new("(send (const {nil? cbase} :Hash) :[] ...)");

#[derive(Default)]
pub struct HashConversion {
    ignored: IgnoredNodes,
}

impl Cop for HashConversion {
    fn name(&self) -> &'static str {
        "Style/HashConversion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.part_of(node) || !HASH_FROM_ARRAY.is_match(node) {
            return;
        }
        // If there is one argument:
        //   Hash[ary] => ary.to_h
        //   Hash[*ary] => don't suggest corrections
        // If there is 0 or 2+ arguments:
        //   Hash[a1, a2, a3, a4] => {a1 => a2, a3 => a4}
        //   ...but don't suggest correction if there is odd number of them (it is a bug)
        if node.arguments().len() == 1 {
            single_argument(cx, node);
        } else {
            multi_argument(cx, node);
        }
        self.ignored.ignore(node);
    }
}

fn single_argument(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let first_argument = node.first_argument().unwrap();
    if first_argument.is(NodeType::Hash) {
        register_offense_for_hash(cx, node, first_argument);
    } else if first_argument.is_any(&[NodeType::Splat, NodeType::ForwardedRestarg]) {
        if !cx.cop_config_bool("AllowSplatArgument", true) {
            cx.add_offense(node.source_range(), MSG_SPLAT);
        }
    } else if use_zip_method_without_argument(first_argument) {
        register_offense_for_zip_method(cx, node, first_argument);
    } else {
        cx.add_offense_with_correction(node.source_range(), MSG_TO_H, |corrector| {
            let mut replacement = first_argument.source().to_owned();
            if requires_parens(first_argument) {
                replacement = format!("({replacement})");
            }
            corrector.replace(node.source_range(), format!("{replacement}.to_h"));
        });
    }
}

fn use_zip_method_without_argument(first_argument: Node<'_>) -> bool {
    first_argument.is(NodeType::Send)
        && first_argument.is_method("zip")
        && first_argument.arguments().is_empty()
}

fn register_offense_for_hash(cx: &mut Investigation<'_, '_>, node: Node<'_>, hash_argument: Node<'_>) {
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(node.source_range(), MSG_LITERAL_HASH_ARG, |corrector| {
        corrector.replace(node.source_range(), format!("{{{}}}", hash_argument.source()));
        if let Some(parent) = node.parent()
            && parent.is(NodeType::Send)
            && !parent.is_parenthesized()
        {
            add_parentheses(src, parent, corrector);
        }
    });
}

fn register_offense_for_zip_method(cx: &mut Investigation<'_, '_>, node: Node<'_>, zip_method: Node<'_>) {
    cx.add_offense_with_correction(node.source_range(), MSG_TO_H, |corrector| {
        if zip_method.is_parenthesized() {
            corrector.insert_before(zip_method.loc().end.unwrap(), "[]");
        } else {
            corrector.insert_after(zip_method.source_range(), "([])");
        }
    });
}

fn requires_parens(node: Node<'_>) -> bool {
    if node.is_call() {
        if node.is_method("[]") {
            return false;
        }
        if !node.arguments().is_empty() && !node.is_parenthesized() {
            return true;
        }
    }
    node.is_operator_keyword()
}

fn multi_argument(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    // A splat argument can expand to any number of elements, so the pairs
    // can't be built statically and there is no literal hash to suggest.
    let arguments = node.arguments();
    if arguments.iter().any(|a| a.is(NodeType::Splat)) {
        return;
    }
    if arguments.len() % 2 == 1 {
        cx.add_offense(node.source_range(), MSG_LITERAL_MULTI_ARG);
    } else {
        correct_multi_argument(cx, node);
    }
}

fn correct_multi_argument(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(node.source_range(), MSG_LITERAL_MULTI_ARG, |corrector| {
        corrector.replace(node.source_range(), args_to_hash(&node.arguments()));
        if let Some(parent) = node.parent()
            && parent.is(NodeType::Send)
            && !parent.is_method("to_h")
            && !parent.is_parenthesized()
        {
            add_parentheses(src, parent, corrector);
        }
    });
}

fn args_to_hash(args: &[Node<'_>]) -> String {
    let content: Vec<String> = args
        .chunks(2)
        .map(|pair| format!("{} => {}", pair[0].source(), pair[1].source()))
        .collect();
    format!("{{{}}}", content.join(", "))
}
