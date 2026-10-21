//! Port of `lib/rubocop/cop/style/signal_exception.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const FAIL_MSG: &str = "Use `fail` instead of `raise` to signal exceptions.";
const RAISE_MSG: &str = "Use `raise` instead of `fail` to rethrow exceptions.";

static KERNEL_CALL: NodePattern = NodePattern::new("(send (const {nil? cbase} :Kernel) %1 ...)");
static CUSTOM_FAIL_METHODS: NodePattern = NodePattern::new("{(def :fail ...) (defs _ :fail ...)}");

#[derive(Default)]
pub struct SignalException {
    ignored: IgnoredNodes,
    custom_fail_defined: Option<bool>,
}

impl Cop for SignalException {
    fn name(&self) -> &'static str {
        "Style/SignalException"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Rescue, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["raise", "fail"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        // `ignored_node?` compares node identity, which never matches nodes
        // of an earlier parse.
        self.ignored = IgnoredNodes::default();
        self.custom_fail_defined = None;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        if node.is(NodeType::Rescue) {
            if style != "semantic" {
                return;
            }
            self.check_scope(cx, "raise", node.body());
            for rescue_node in node.resbody_branches() {
                self.check_scope(cx, "fail", Some(rescue_node));
                for send_node in each_command_or_kernel_call("raise", rescue_node) {
                    self.ignored.ignore(send_node);
                }
            }
            return;
        }
        match style {
            "semantic" => {
                if !self.ignored.contains(node) {
                    check_send(cx, "raise", node);
                }
            }
            "only_raise" => {
                if self.custom_fail_defined(cx) {
                    return;
                }
                check_send(cx, "fail", node);
            }
            "only_fail" => check_send(cx, "raise", node),
            _ => {}
        }
    }
}

impl SignalException {
    fn custom_fail_defined(&mut self, cx: &Investigation<'_, '_>) -> bool {
        *self.custom_fail_defined.get_or_insert_with(|| {
            cx.processed_source
                .ast()
                .is_some_and(|ast| CUSTOM_FAIL_METHODS.any_in(ast))
        })
    }

    fn check_scope<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        method_name: &str,
        node: Option<Node<'a>>,
    ) {
        let Some(node) = node else {
            return;
        };
        for send_node in each_command_or_kernel_call(method_name, node) {
            if self.ignored.contains(send_node) {
                continue;
            }
            let message = message(cx, method_name);
            let style = style(cx);
            cx.add_offense_with_correction(send_node.loc().selector.unwrap(), message, |corrector| {
                corrector.replace(send_node.loc().selector.unwrap(), correction(style, send_node));
            });
            self.ignored.ignore(send_node);
        }
    }
}

fn message(cx: &Investigation<'_, '_>, method_name: &str) -> &'static str {
    match style(cx) {
        "semantic" => {
            if method_name == "fail" {
                RAISE_MSG
            } else {
                FAIL_MSG
            }
        }
        "only_raise" => "Always use `raise` to signal exceptions.",
        "only_fail" => "Always use `fail` to signal exceptions.",
        _ => "",
    }
}

fn check_send(cx: &mut Investigation<'_, '_>, method_name: &str, node: Node<'_>) {
    if !command_or_kernel_call(method_name, node) {
        return;
    }
    let message = message(cx, method_name);
    let style = style(cx);
    let selector = node.loc().selector.unwrap();
    cx.add_offense_with_correction(selector, message, |corrector| {
        corrector.replace(selector, correction(style, node));
    });
}

fn correction(style: &str, node: Node<'_>) -> &'static str {
    match style {
        "semantic" => {
            if command_or_kernel_call("raise", node) {
                "fail"
            } else {
                "raise"
            }
        }
        "only_raise" => "raise",
        _ => "fail",
    }
}

fn command_or_kernel_call(name: &str, node: Node<'_>) -> bool {
    if !node.is_method(name) {
        return false;
    }
    let params = [Param::Sym(name)];
    let ctx = MatchContext {
        params: &params,
        ..MatchContext::default()
    };
    node.is_command(name) || KERNEL_CALL.match_with(node, &ctx).is_some()
}

/// `on_node(:send, node, :rescue)` filtered by `command_or_kernel_call?`.
fn each_command_or_kernel_call<'a>(method_name: &str, node: Node<'a>) -> Vec<Node<'a>> {
    fn walk<'a>(method_name: &str, node: Node<'a>, out: &mut Vec<Node<'a>>) {
        if node.is_send() && command_or_kernel_call(method_name, node) {
            out.push(node);
        }
        if node.is(NodeType::Rescue) {
            return;
        }
        for child in node.child_nodes() {
            walk(method_name, child, out);
        }
    }
    let mut out = Vec::new();
    walk(method_name, node, &mut out);
    out
}
