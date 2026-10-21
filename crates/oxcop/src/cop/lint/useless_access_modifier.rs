//! Port of `lib/rubocop/cop/lint/useless_access_modifier.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

static STATIC_METHOD_DEFINITION: NodePattern =
    NodePattern::new("{def (send nil? {:attr :attr_reader :attr_writer :attr_accessor} ...)}");
static DYNAMIC_METHOD_DEFINITION: NodePattern = NodePattern::new(
    "{(send nil? :define_method ...) (any_block (send nil? :define_method ...) ...)}",
);
static CLASS_OR_INSTANCE_EVAL: NodePattern =
    NodePattern::new("(any_block (send _ {:class_eval :instance_eval}) ...)");

#[derive(Default)]
pub struct UselessAccessModifier;

impl Cop for UselessAccessModifier {
    fn name(&self) -> &'static str {
        "Lint/UselessAccessModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Class,
            NodeType::Module,
            NodeType::Sclass,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Begin,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Class | NodeType::Module | NodeType::Sclass => check_node(cx, node.body()),
            NodeType::Begin => on_begin(cx, node),
            _ => {
                if !(eval_call(cx, node) || included_block(cx, node)) {
                    return;
                }
                check_node(cx, node.body());
            }
        }
    }
}

fn on_begin(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if node.parent().is_some() {
        return;
    }
    for child in node.child_nodes() {
        if !(child.is(NodeType::Send) && access_modifier(child)) {
            continue;
        }
        // `check_send_node(child, child.method_name, true)`: the current
        // visibility is the modifier's own, so it is always useless.
        if child.is_bare_access_modifier()
            || (child.is_method("private_class_method") && !child.has_arguments())
        {
            add_useless(cx, child, child.method_name().unwrap_or_default());
        }
    }
}

fn add_useless(cx: &mut Investigation<'_, '_>, node: Node<'_>, current: &str) {
    let ps = cx.processed_source;
    let message = format!("Useless `{current}` access modifier.");
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        let range = range_by_whole_lines(ps, node.source_range(), true);
        corrector.remove(range);
    });
}

fn check_node(cx: &mut Investigation<'_, '_>, node: Option<Node<'_>>) {
    let Some(node) = node else {
        return;
    };

    if node.is(NodeType::Begin) {
        check_scope(cx, node);
    } else if node.is(NodeType::Send) && node.is_bare_access_modifier() {
        add_useless(cx, node, node.method_name().unwrap_or_default());
    }
}

fn access_modifier(node: Node<'_>) -> bool {
    node.is_bare_access_modifier() || node.is_method("private_class_method")
}

fn check_scope(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let (cur_vis, unused) = check_child_nodes(cx, node, None, Some("public"));
    let Some(unused) = unused else {
        return;
    };
    add_useless(cx, unused, cur_vis.unwrap_or_default());
}

type State<'a, 'v> = (Option<&'v str>, Option<Node<'a>>);

fn check_child_nodes<'a, 'v>(
    cx: &mut Investigation<'_, '_>,
    node: Node<'a>,
    mut unused: Option<Node<'a>>,
    mut cur_vis: Option<&'v str>,
) -> State<'a, 'v>
where
    'a: 'v,
{
    for child in node.child_nodes() {
        if child.is(NodeType::Send) && access_modifier(child) {
            (cur_vis, unused) = check_send_node(cx, child, cur_vis, unused);
        } else if child.is(NodeType::Block) && included_block(cx, child) {
            continue;
        } else if method_definition(cx, child) {
            unused = None;
        } else if start_of_new_scope(cx, child) {
            check_scope(cx, child);
        } else if !child.is(NodeType::Defs) {
            (cur_vis, unused) = check_child_nodes(cx, child, unused, cur_vis);
        }
    }
    (cur_vis, unused)
}

fn check_send_node<'a, 'v>(
    cx: &mut Investigation<'_, '_>,
    node: Node<'a>,
    cur_vis: Option<&'v str>,
    unused: Option<Node<'a>>,
) -> State<'a, 'v>
where
    'a: 'v,
{
    if node.is_bare_access_modifier() {
        check_new_visibility(cx, node, unused, node.method_name(), cur_vis)
    } else if node.is_method("private_class_method") && !node.has_arguments() {
        add_useless(cx, node, node.method_name().unwrap_or_default());
        (cur_vis, unused)
    } else {
        (None, None)
    }
}

fn check_new_visibility<'a, 'v>(
    cx: &mut Investigation<'_, '_>,
    node: Node<'a>,
    mut unused: Option<Node<'a>>,
    new_vis: Option<&'a str>,
    cur_vis: Option<&'v str>,
) -> State<'a, 'v>
where
    'a: 'v,
{
    if new_vis == cur_vis {
        add_useless(cx, node, cur_vis.unwrap_or_default());
    } else {
        if let Some(unused) = unused {
            add_useless(cx, unused, cur_vis.unwrap_or_default());
        }
        unused = Some(node);
    }
    (new_vis, unused)
}

fn included_block(cx: &Investigation<'_, '_>, block_node: Node<'_>) -> bool {
    cx.config.all_cops_bool("ActiveSupportExtensionsEnabled", false)
        && block_node.is_method("included")
}

fn method_definition(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    STATIC_METHOD_DEFINITION.is_match(child)
        || DYNAMIC_METHOD_DEFINITION.is_match(child)
        || any_method_definition(cx, child)
}

/// `{def (send nil? :<m> ...)}` for each of `MethodCreatingMethods`.
fn any_method_definition(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    cx.cop_config_list("MethodCreatingMethods")
        .into_iter()
        .any(|m| {
            if m == "included" {
                return false;
            }
            child.is(NodeType::Def)
                || (child.is(NodeType::Send)
                    && child.receiver().is_none()
                    && child.method_name() == Some(m))
        })
}

fn start_of_new_scope(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    matches!(
        child.ty(),
        NodeType::Module | NodeType::Class | NodeType::Sclass
    ) || eval_call(cx, child)
}

fn eval_call(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    CLASS_OR_INSTANCE_EVAL.is_match(child)
        || child.is_class_constructor()
        || any_context_creating_methods(cx, child)
}

/// `(any_block (send {nil? const} {:<m>} ...) ...)` for each of
/// `ContextCreatingMethods`.
fn any_context_creating_methods(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    cx.cop_config_list("ContextCreatingMethods")
        .into_iter()
        .any(|m| {
            if m == "included" {
                return false;
            }
            child.is_any_block()
                && child.child(0).is_some_and(|send| {
                    send.is(NodeType::Send)
                        && send
                            .child(0)
                            .is_none_or(|receiver| receiver.is(NodeType::Const))
                        && send.method_name() == Some(m)
                })
        })
}
