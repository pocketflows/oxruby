//! Port of `lib/rubocop/cop/style/reduce_to_hash.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const RESTRICT_ON_SEND: &[&str] = &["each_with_object", "inject", "reduce"];

static EACH_WITH_OBJECT_TO_HASH: NodePattern = NodePattern::new(
    "{
      (block
        (call _ :each_with_object (hash))
        (args (arg _elem) (arg _hash))
        (send (lvar _hash) :[]= $_key $_value))
      (numblock
        (call _ :each_with_object (hash))
        2
        (send (lvar :_2) :[]= $_key $_value))
    }",
);

static INJECT_TO_HASH: NodePattern = NodePattern::new(
    "{
      (block
        (call _ {:inject :reduce} (hash))
        (args (arg _hash) (arg _elem))
        (begin
          (send (lvar _hash) :[]= $_key $_value)
          (lvar _hash)))
      (numblock
        (call _ {:inject :reduce} (hash))
        2
        (begin
          (send (lvar :_1) :[]= $_key $_value)
          (lvar :_1)))
    }",
);

#[derive(Default)]
pub struct ReduceToHash;

impl Cop for ReduceToHash {
    fn name(&self) -> &'static str {
        "Style/ReduceToHash"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(block_node) = node.block_node() else {
            return;
        };
        let Some((key, value)) = to_hash_match(node, block_node) else {
            return;
        };
        if accumulator_used_in_expressions(block_node, key, value)
            || nested_match(key)
            || nested_match(value)
        {
            return;
        }
        let message = format!(
            "Use `to_h {{ ... }}` instead of `{}`.",
            node.method_name().unwrap_or("")
        );
        let selector = node.loc().selector.unwrap();
        cx.add_offense_with_correction(selector, &message, |corrector| {
            let range = range_between(selector.begin_pos, block_node.source_range().end_pos);
            corrector.replace(range, replacement(block_node, key, value));
        });
    }
}

fn to_hash_match<'a>(send_node: Node<'a>, block_node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let pattern = if send_node.is_method("each_with_object") {
        &EACH_WITH_OBJECT_TO_HASH
    } else {
        &INJECT_TO_HASH
    };
    let c = pattern.match_node(block_node)?;
    Some((c[0].node()?, c[1].node()?))
}

fn accumulator_used_in_expressions(block_node: Node<'_>, key: Node<'_>, value: Node<'_>) -> bool {
    let index = if block_node.is_method("each_with_object") {
        1
    } else {
        0
    };
    let names = block_node.block_argument_names();
    let Some(acc_name) = names.get(index) else {
        return false;
    };
    references_variable(key, acc_name) || references_variable(value, acc_name)
}

fn nested_match(node: Node<'_>) -> bool {
    node.each_node().filter(|n| n.is_call()).any(|send_node| {
        if !send_node
            .method_name()
            .is_some_and(|m| RESTRICT_ON_SEND.contains(&m))
        {
            return false;
        }
        let Some(inner_block) = send_node.block_node() else {
            return false;
        };
        to_hash_match(send_node, inner_block).is_some()
    })
}

fn references_variable(node: Node<'_>, name: &str) -> bool {
    node.each_node()
        .any(|lvar| lvar.is(NodeType::Lvar) && lvar.child_sym(0) == Some(name))
}

fn replacement(block_node: Node<'_>, key_expr: Node<'_>, value_expr: Node<'_>) -> String {
    let key_source = adjusted_source(key_expr, block_node);
    let value_source = adjusted_source(value_expr, block_node);
    let body = format!("[{key_source}, {value_source}]");
    if block_node.is(NodeType::Numblock) {
        if block_node.has_braces() {
            format!("to_h {{ {body} }}")
        } else {
            do_end_replacement(block_node, &body, None)
        }
    } else {
        let arg = element_arg_source(block_node);
        if block_node.has_braces() {
            format!("to_h {{ |{arg}| {body} }}")
        } else {
            do_end_replacement(block_node, &body, Some(arg))
        }
    }
}

fn do_end_replacement(block_node: Node<'_>, body: &str, arg: Option<&str>) -> String {
    let args = arg.map_or(String::new(), |a| format!(" |{a}|"));
    let indent = " ".repeat(block_node.column());
    format!("to_h do{args}\n{indent}  {body}\n{indent}end")
}

fn element_arg_source<'a>(block_node: Node<'a>) -> &'a str {
    let arguments = block_node.arguments();
    if block_node.is_method("each_with_object") {
        arguments[0].source()
    } else {
        arguments[1].source()
    }
}

fn adjusted_source(expr_node: Node<'_>, block_node: Node<'_>) -> String {
    let source = expr_node.source();
    if !block_node.is(NodeType::Numblock) || block_node.is_method("each_with_object") {
        return source.to_owned();
    }
    // For inject/reduce numblocks, _2 is the element (becomes _1)
    source.replace("_2", "_1")
}
