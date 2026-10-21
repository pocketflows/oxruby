//! Port of `lib/rubocop/cop/naming/method_name.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::configurable_naming::{
    forbidden_identifier, forbidden_pattern, naming_format, valid_name,
};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const OPERATOR_METHODS: &[&str] = &[
    "|", "^", "&", "<=>", "==", "===", "=~", ">", ">=", "<", "<=", "<<", ">>", "+", "-", "*", "/",
    "%", "**", "~", "+@", "-@", "!@", "~@", "[]", "[]=", "!", "!=", "!~", "`",
];

#[derive(Default)]
pub struct MethodName;

impl Cop for MethodName {
    fn name(&self) -> &'static str {
        "Naming/MethodName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Def, NodeType::Defs, NodeType::Alias]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "define_method",
            "define_singleton_method",
            "new",
            "define",
            "alias_method",
            "attr",
            "attr_reader",
            "attr_writer",
            "attr_accessor",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Send => on_send(cx, node),
            NodeType::Alias => {
                let new_identifier = node.child(0).unwrap();
                if new_identifier.is(NodeType::Sym) {
                    handle_method_name(cx, new_identifier, new_identifier.sym_value());
                }
            }
            _ => {
                let name = node.method_name().unwrap_or("");
                if node.is_operator_method() || matches_allowed_pattern(cx, name) {
                    return;
                }
                if forbidden_name(cx, name) {
                    register_forbidden_name(cx, node);
                } else {
                    check_name(cx, node, name, node.loc().name.unwrap());
                }
            }
        }
    }
}

fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let global = |name: &str| node.receiver().is_some_and(|r| r.is_global_const(name));
    if node.is_method("define_method") || node.is_method("define_singleton_method") {
        if let Some(first) = node.first_argument()
            && matches!(first.ty(), NodeType::Str | NodeType::Sym)
        {
            handle_method_name(cx, node, value(first));
        }
    } else if node.is_method("new") && global("Struct") {
        let args = node.arguments();
        let args = if node.first_argument().is_some_and(|a| a.is(NodeType::Str)) {
            &args[1..]
        } else {
            &args[..]
        };
        for arg in args.iter().filter(|a| matches!(a.ty(), NodeType::Sym | NodeType::Str)) {
            handle_method_name(cx, *arg, value(*arg));
        }
    } else if node.is_method("define") && global("Data") {
        for arg in node
            .arguments()
            .iter()
            .filter(|a| matches!(a.ty(), NodeType::Sym | NodeType::Str))
        {
            handle_method_name(cx, *arg, value(*arg));
        }
    } else if node.is_method("alias_method") {
        let args = node.arguments();
        if args.len() != 2 || !matches!(args[0].ty(), NodeType::Str | NodeType::Sym) {
            return;
        }
        handle_method_name(cx, args[0], value(args[0]));
    } else if let Some((_, attrs)) = node.attribute_accessor() {
        for item in attrs {
            let Some(name) = attr_name(item) else {
                continue;
            };
            if matches_allowed_pattern(cx, name) {
                continue;
            }
            if forbidden_name(cx, name) {
                register_forbidden_name(cx, node);
            } else {
                check_name(cx, node, name, range_position(node));
            }
        }
    }
}

fn value(node: Node<'_>) -> Option<&str> {
    match node.children().first() {
        Some(Child::Sym(s) | Child::Str(s)) => Some(s),
        _ => None,
    }
}

fn attr_name(node: Node<'_>) -> Option<&str> {
    if matches!(node.ty(), NodeType::Sym | NodeType::Str) && node.children().len() == 1 {
        value(node)
    } else {
        None
    }
}

fn handle_method_name<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, name: Option<&str>) {
    let Some(name) = name else {
        return;
    };
    if matches_allowed_pattern(cx, name) {
        return;
    }
    if forbidden_name(cx, name) {
        register_forbidden_name(cx, node);
    } else if !OPERATOR_METHODS.contains(&name) {
        check_name(cx, node, name, range_position(node));
    }
}

fn forbidden_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    forbidden_identifier(cx, name) || forbidden_pattern(cx, name)
}

fn register_forbidden_name<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let (range, name) = if node.is_any_def() {
        (node.loc().name.unwrap(), node.method_name().unwrap_or(""))
    } else if node.is_literal() {
        (node.source_range(), value(node).unwrap_or(""))
    } else if let Some((_, attrs)) = node.attribute_accessor() {
        let last = *attrs.last().unwrap();
        (last.source_range(), attr_name(last).unwrap_or(""))
    } else {
        let first = node.first_argument().unwrap();
        (first.source_range(), value(first).unwrap_or(""))
    };
    let message = format!("`{name}` is forbidden, use another method name instead.");
    cx.add_offense(range, &message);
}

fn range_position(node: Node<'_>) -> Range {
    match node.loc().selector {
        Some(selector) => Range::new(selector.end_pos + 1, node.source_range().end_pos),
        None => node.source_range(),
    }
}

fn check_name<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, name: &str, range: Range) {
    let style = style(cx);
    if valid_name(node, name, naming_format(style)) {
        return;
    }
    let message = format!("Use {style} for method names.");
    cx.add_offense(range, &message);
}
