//! Port of `lib/rubocop/cop/style/hash_each_methods.rb`.
//!
//! The cop includes `Lint::UnusedArgument` only for its `after_leaving_scope`
//! hook, which hands every variable to the cop's own `check_argument`; that
//! is done here directly.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_receivers::allowed_receiver;
use crate::cop::variable_force::{Scope, VariableTable};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const ARRAY_CONVERTER_METHODS: &[&str] =
    &["assoc", "chunk", "flatten", "rassoc", "sort", "sort_by", "to_a"];

static KV_EACH: NodePattern =
    NodePattern::new("(any_block $(call (call _ ${:keys :values}) :each) ...)");
static EACH_ARGUMENTS: NodePattern =
    NodePattern::new("(block (call _ :each)(args $_key $_value) ...)");
static KV_EACH_WITH_BLOCK_PASS: NodePattern =
    NodePattern::new("(call $(call _ ${:keys :values}) :each (block_pass (sym _)))");
static HASH_MUTATED: NodePattern = NodePattern::new("`(send %1 :[]= ...)");

#[derive(Default)]
pub struct HashEachMethods {
    /// `@block_args`: each block argument's declaration range and `used?`.
    block_args: Vec<(Range, bool)>,
}

impl Cop for HashEachMethods {
    fn name(&self) -> &'static str {
        "Style/HashEachMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn joins_variable_force(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::BlockPass,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.block_args.clear();
    }

    fn after_leaving_scope<'a>(
        &mut self,
        _cx: &mut Investigation<'a, '_>,
        scope: &Scope<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        for variable in variable_table.scope_variables_of(scope) {
            // `check_argument(variable)`
            if variable.is_block_argument() {
                self.block_args
                    .push((variable.declaration_node.source_range(), variable.is_used()));
            }
        }
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::BlockPass) {
            self.on_block_pass(cx, node);
        } else {
            self.on_block(cx, node);
        }
    }
}

impl HashEachMethods {
    fn on_block<'a>(&self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !handleable(node) {
            return;
        }
        if let Some(captures) = KV_EACH.match_node(node) {
            let target = captures[0].node().unwrap();
            let method = captures[1].sym().unwrap_or("");
            if self.register_kv_offense(cx, target, method) {
                return;
            }
        }
        let Some(captures) = EACH_ARGUMENTS.match_node(node) else {
            return;
        };
        let (Some(key), Some(value)) = (captures[0].node(), captures[1].node()) else {
            return;
        };
        check_unused_block_args(cx, node, key, value);
    }

    fn on_block_pass<'a>(&self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent) = node.parent() else {
            return;
        };
        let Some(captures) = KV_EACH_WITH_BLOCK_PASS.match_node(parent) else {
            return;
        };
        let target = captures[0].node().unwrap();
        let method = captures[1].sym().unwrap_or("");
        let Some(parent_receiver) = parent.receiver().and_then(|r| r.receiver()) else {
            return;
        };
        if allowed_receiver(cx, parent_receiver) {
            return;
        }
        let range = target
            .loc()
            .selector
            .unwrap()
            .join(parent.loc().selector.unwrap().end());
        let message = format_message(method, cx.source(range));
        let replacement = format!("each_{}", &method[..method.len() - 1]);
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &replacement);
        });
    }

    /// Whether an offense was added (Ruby's `add_offense` result).
    fn register_kv_offense(&self, cx: &mut Investigation<'_, '_>, target: Node<'_>, method: &str) -> bool {
        let Some(receiver) = target.receiver() else {
            return false;
        };
        let Some(parent_receiver) = receiver.receiver() else {
            return false;
        };
        if allowed_receiver(cx, parent_receiver) {
            return false;
        }
        let current = cx.source(
            receiver
                .loc()
                .selector
                .unwrap()
                .join(target.source_range().end()),
        );
        let range = kv_range(target);
        if cx.has_offense_at(range) {
            return false;
        }
        let message = format_message(method, current);
        let block_args = &self.block_args;
        cx.add_offense_with_correction(range, &message, |corrector| {
            correct_key_value_each(block_args, target, corrector);
        });
        true
    }
}

fn check_unused_block_args(cx: &mut Investigation<'_, '_>, node: Node<'_>, key: Node<'_>, value: Node<'_>) {
    if node.body().is_none() {
        return;
    }
    let value_unused = unused_block_arg_exist(node, value);
    let key_unused = unused_block_arg_exist(node, key);
    if value_unused && key_unused {
        return;
    }
    let method_name = node.method_name().unwrap_or("");
    if value_unused {
        let message = message("each_key", method_name, value.source());
        let unused_range = key
            .source_range()
            .end()
            .join(value.source_range().end());
        register_each_args_offense(cx, node, &message, "each_key", unused_range);
    } else if key_unused {
        let message = message("each_value", method_name, key.source());
        let unused_range = key
            .source_range()
            .begin()
            .join(value.source_range().begin());
        register_each_args_offense(cx, node, &message, "each_value", unused_range);
    }
}

fn handleable(node: Node<'_>) -> bool {
    if use_array_converter_method_as_preceding(node) {
        return false;
    }
    let Some(root_receiver) = root_receiver(node) else {
        return false;
    };
    let ctx = MatchContext {
        params: &[Param::Node(root_receiver)],
        ..Default::default()
    };
    if HASH_MUTATED.match_with(node, &ctx).is_some() {
        return false;
    }
    !root_receiver.is_literal() || root_receiver.is(NodeType::Hash)
}

fn unused_block_arg_exist(node: Node<'_>, block_arg: Node<'_>) -> bool {
    let lvar_sources: Vec<&str> = node
        .body()
        .map(|body| {
            body.descendants()
                .filter(|d| d.is(NodeType::Lvar))
                .map(|d| d.source())
                .collect()
        })
        .unwrap_or_default();
    let unused = |source: &str| {
        let name = source.strip_prefix('*').unwrap_or(source);
        !lvar_sources.contains(&name)
    };
    if block_arg.is(NodeType::Mlhs) {
        block_arg
            .descendants()
            .filter(|d| d.is_any(&[NodeType::Arg, NodeType::Restarg]))
            .all(|d| unused(d.source()))
    } else {
        unused(block_arg.source())
    }
}

fn message(prefer: &str, method_name: &str, unused_code: &str) -> String {
    format!(
        "Use `{prefer}` instead of `{method_name}` and remove the unused `{unused_code}` block argument."
    )
}

fn register_each_args_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    message: &str,
    prefer: &str,
    unused_range: Range,
) {
    cx.add_offense_with_correction(node.source_range(), message, |corrector| {
        corrector.replace(node.send_node().unwrap().loc().selector.unwrap(), prefer);
        corrector.remove(unused_range);
    });
}

fn use_array_converter_method_as_preceding(node: Node<'_>) -> bool {
    let Some(preceding_method) = node.child(0).and_then(|send| send.child(0)) else {
        return false;
    };
    if !(preceding_method.is_call() || preceding_method.is_any_block()) {
        return false;
    }
    preceding_method
        .method_name()
        .is_some_and(|m| ARRAY_CONVERTER_METHODS.contains(&m))
}

fn root_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver();
    match receiver {
        Some(r) if r.receiver().is_some() => root_receiver(r),
        _ => receiver,
    }
}

fn format_message(method_name: &str, current: &str) -> String {
    format!(
        "Use `each_{}` instead of `{current}`.",
        &method_name[..method_name.len() - 1]
    )
}

fn correct_key_value_each(block_args: &[(Range, bool)], node: Node<'_>, corrector: &mut Corrector) {
    let receiver = node.receiver().unwrap();
    let method = receiver.method_name().unwrap_or("");
    let name = format!("each_{}", &method[..method.len().saturating_sub(1)]);
    match receiver.receiver() {
        None => {
            // `correct_implicit`
            corrector.replace(node.source_range(), &name);
            correct_args(block_args, node, corrector);
        }
        Some(outer) => {
            let dot = node.range_source(node.loc().dot.unwrap());
            corrector.replace(node.source_range(), format!("{}{dot}{name}", outer.source()));
        }
    }
}

fn correct_args(block_args: &[(Range, bool)], node: Node<'_>, corrector: &mut Corrector) {
    let Some(args) = node.parent().and_then(|p| p.child(1)) else {
        return;
    };
    let used = |arg: Node<'_>| {
        block_args
            .iter()
            .find(|(range, _)| *range == arg.source_range())
            .is_some_and(|(_, used)| *used)
    };
    let name = args
        .child_nodes()
        .find(|arg| used(*arg))
        .and_then(|arg| arg.child_sym(0))
        .unwrap_or("");
    corrector.replace(args.source_range(), format!("|{name}|"));
}

fn kv_range(outer_node: Node<'_>) -> Range {
    outer_node
        .receiver()
        .unwrap()
        .loc()
        .selector
        .unwrap()
        .join(outer_node.loc().selector.unwrap())
}
