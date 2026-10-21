//! Port of `lib/rubocop/cop/lint/number_conversion.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Corrector, Investigation};

const CONVERSION_METHODS: &[&str] = &[
    "Integer", "Float", "Complex", "Rational", "to_i", "to_f", "to_c", "to_r",
];

static TO_METHOD: NodePattern = NodePattern::new("(call $_ ${:to_i :to_f :to_c :to_r})");
static TO_METHOD_SYMBOL: NodePattern = NodePattern::new(
    "(call _ $_ ${
      {
        (sym ${:to_i :to_f :to_c :to_r})
        (block_pass (sym ${:to_i :to_f :to_c :to_r}))
      }
    } ...)",
);

fn conversion_method_class_mapping(method: &str, number_object: &str) -> Option<String> {
    Some(match method {
        "to_i" => format!("Integer({number_object}, 10)"),
        "to_f" => format!("Float({number_object})"),
        "to_c" => format!("Complex({number_object})"),
        "to_r" => format!("Rational({number_object})"),
        _ => return None,
    })
}

fn message(current: &str, corrected_method: &str) -> String {
    format!(
        "Replace unsafe number conversion with number class parsing, instead of using `{current}`, use stricter `{corrected_method}`."
    )
}

#[derive(Default)]
pub struct NumberConversion {
    ignored: IgnoredNodes,
}

impl Cop for NumberConversion {
    fn name(&self) -> &'static str {
        "Lint/NumberConversion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        self.handle_conversion_method(cx, node);
        handle_as_symbol(cx, node);
    }
}

impl NumberConversion {
    fn handle_conversion_method<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = TO_METHOD.match_node(node) else {
            return;
        };
        let Some(receiver) = captures[0].node() else {
            return;
        };
        if allow_receiver(cx, receiver) {
            return;
        }
        let to_method = captures[1].sym().unwrap();
        let message = message(
            &current_method(node, receiver, to_method),
            &correct_method(node, receiver),
        );
        let ignored = &mut self.ignored;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if safe_navigation(node) || ignored.part_of(node) {
                return;
            }
            corrector.replace(
                node.source_range(),
                correct_method(node, node.receiver().unwrap()),
            );
            ignored.ignore(node);
        });
    }
}

fn handle_as_symbol<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(captures) = TO_METHOD_SYMBOL.match_node(node) else {
        return;
    };
    if node.arguments().len() != 1 {
        return;
    }
    let sym_node = captures[1].node().unwrap();
    let to_method = captures[2].sym().unwrap();
    let message = message(sym_node.source(), &correct_sym_method(to_method));
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        if node.is_parenthesized() {
            remove_parentheses(corrector, node);
        }
        corrector.replace(sym_node.source_range(), correct_sym_method(to_method));
    });
}

fn correct_method(node: Node<'_>, receiver: Node<'_>) -> String {
    conversion_method_class_mapping(node.method_name().unwrap(), receiver.source()).unwrap()
}

fn correct_sym_method(to_method: &str) -> String {
    let body = conversion_method_class_mapping(to_method, "i").unwrap();
    format!("{{ |i| {body} }}")
}

fn current_method(node: Node<'_>, receiver: Node<'_>, to_method: &str) -> String {
    let operator = if node.is_csend() { "&." } else { "." };
    format!("{}{operator}{to_method}", receiver.source())
}

fn remove_parentheses(corrector: &mut Corrector, node: Node<'_>) {
    corrector.replace(node.loc().begin.unwrap(), " ");
    corrector.remove(node.loc().end.unwrap());
}

fn safe_navigation(node: Node<'_>) -> bool {
    node.is_csend() || node.descendants().any(|d| d.is(NodeType::Csend))
}

fn allow_receiver(cx: &Investigation<'_, '_>, receiver: Node<'_>) -> bool {
    if receiver.is_numeric()
        || (receiver.is_call()
            && receiver.method_name().is_some_and(|name| {
                conversion_method(name) || allowed_method_name(cx, name)
            }))
    {
        true
    } else {
        let receiver = top_receiver(receiver);
        receiver.is(NodeType::Const)
            && receiver
                .const_name()
                .is_some_and(|name| ignored_class(cx, &name))
    }
}

fn allowed_method_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    allowed_method(cx, name) || matches_allowed_pattern(cx, name)
}

fn top_receiver(node: Node<'_>) -> Node<'_> {
    let mut receiver = node;
    while let Some(next) = receiver.receiver() {
        receiver = next;
    }
    receiver
}

fn conversion_method(method_name: &str) -> bool {
    CONVERSION_METHODS.contains(&method_name)
}

fn ignored_class(cx: &Investigation<'_, '_>, name: &str) -> bool {
    cx.cop_config_list("AllowedClasses").contains(&name)
}
