//! Port of `lib/rubocop/cop/naming/memoized_instance_variable_name.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style_for;
use crate::cop::{Cop, Investigation};
use crate::regex;

const DYNAMIC_DEFINE_METHODS: &[&str] = &["define_method", "define_singleton_method"];
const INITIALIZE_METHODS: &[&str] = &[
    "initialize",
    "initialize_clone",
    "initialize_copy",
    "initialize_dup",
];

#[derive(Default)]
pub struct MemoizedInstanceVariableName;

impl Cop for MemoizedInstanceVariableName {
    fn name(&self) -> &'static str {
        "Naming/MemoizedInstanceVariableName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::OrAsgn, NodeType::Defined]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::OrAsgn) {
            on_or_asgn(cx, node);
        } else {
            on_defined(cx, node);
        }
    }
}

fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, "EnforcedStyleForLeadingUnderscores")
}

fn on_or_asgn<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(lhs) = node.lhs() else {
        return;
    };
    if !lhs.is(NodeType::Ivasgn) {
        return;
    }
    let Some((method_node, method_name)) = find_definition(node) else {
        return;
    };
    if !nameable_method(method_name) {
        return;
    }
    let Some(body) = method_node.body() else {
        return;
    };
    let is_last = match body.children().last() {
        Some(Child::Node(id)) => node.ast().node(*id).same_as(node),
        _ => false,
    };
    if !(body.same_as(node) || is_last) {
        return;
    }
    if matches(cx, method_name, Some(lhs)) {
        return;
    }
    let suggested_var = suggested_var(cx, method_name);
    let var = lhs.name().unwrap_or("");
    let message = message(cx, var, &suggested_var, method_name);
    let name_range = lhs.loc().name.unwrap();
    cx.add_offense_with_correction(lhs.source_range(), &message, |corrector| {
        corrector.replace(name_range, format!("@{suggested_var}"));
    });
}

/// `defined_memoized?(body, ivar)`:
/// `(begin (if (defined $(ivar %1)) (return $(ivar %1)) nil?) ... $(ivasgn %1 _))`
fn defined_memoized<'a>(body: Node<'a>, ivar: &str) -> Option<(Node<'a>, Node<'a>, Node<'a>)> {
    if !body.is(NodeType::Begin) || body.children().len() < 2 {
        return None;
    }
    let first = body.child(0)?;
    if !first.is(NodeType::If) || !first.children()[2].is_nil() {
        return None;
    }
    let condition = first.child(0)?;
    if !condition.is(NodeType::Defined) || condition.children().len() != 1 {
        return None;
    }
    let defined_ivar = condition.child(0).filter(|n| is_ivar(*n, ivar))?;
    let ret = first.child(1).filter(|n| n.is(NodeType::Return))?;
    if ret.children().len() != 1 {
        return None;
    }
    let return_ivar = ret.child(0).filter(|n| is_ivar(*n, ivar))?;
    let last = body.child_from_end(1)?;
    if !last.is(NodeType::Ivasgn) || last.name() != Some(ivar) || last.children().len() != 2 {
        return None;
    }
    Some((defined_ivar, return_ivar, last))
}

fn is_ivar(node: Node<'_>, name: &str) -> bool {
    node.is(NodeType::Ivar) && node.name() == Some(name)
}

fn on_defined<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(arg) = node.child(0) else {
        return;
    };
    if !arg.is(NodeType::Ivar) {
        return;
    }
    let Some((method_node, method_name)) = find_definition(node) else {
        return;
    };
    if !nameable_method(method_name) {
        return;
    }
    let Some(body) = method_node.body() else {
        return;
    };
    let var = arg.name().unwrap_or("");
    let Some((defined_ivar, return_ivar, ivar_assign)) = defined_memoized(body, var) else {
        return;
    };
    if matches(cx, method_name, Some(ivar_assign)) {
        return;
    }
    let suggested_var = suggested_var(cx, method_name);
    let message = message(cx, var, &suggested_var, method_name);
    let replacement = format!("@{suggested_var}");
    for range in [
        defined_ivar.source_range(),
        return_ivar.source_range(),
        ivar_assign.loc().name.unwrap(),
    ] {
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &replacement);
        });
    }
}

/// `method_definition?`: the method node and its name.
fn method_definition(node: Node<'_>) -> Option<(Node<'_>, &str)> {
    match node.ty() {
        NodeType::Block => {
            let send = node.child(0).filter(|s| s.is(NodeType::Send))?;
            if !DYNAMIC_DEFINE_METHODS.contains(&send.method_name()?) {
                return None;
            }
            let [_, _, Child::Node(arg)] = send.children() else {
                return None;
            };
            let arg = node.ast().node(*arg);
            if !matches!(arg.ty(), NodeType::Sym | NodeType::Str) {
                return None;
            }
            match arg.children().first() {
                Some(Child::Sym(name) | Child::Str(name)) => Some((node, name.as_str())),
                _ => None,
            }
        }
        NodeType::Def => Some((node, node.child_sym(0)?)),
        NodeType::Defs => Some((node, node.child_sym(1)?)),
        _ => None,
    }
}

fn find_definition(node: Node<'_>) -> Option<(Node<'_>, &str)> {
    // Methods can be defined in a `def` or `defs`, or dynamically via a
    // `block` node.
    node.ancestors()
        .filter(|a| matches!(a.ty(), NodeType::Def | NodeType::Defs | NodeType::Block))
        .find_map(method_definition)
}

fn delete_punctuation(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '!' | '?' | '=')).collect()
}

/// Operator and other non-word method names cannot form a valid instance
/// variable name.
fn nameable_method(method_name: &str) -> bool {
    regex!(r"\A[a-zA-Z_]\w*\z")
        .find(&delete_punctuation(method_name))
        .is_some()
}

fn matches(cx: &Investigation<'_, '_>, method_name: &str, ivar_assign: Option<Node<'_>>) -> bool {
    let Some(ivar_assign) = ivar_assign else {
        return true;
    };
    if INITIALIZE_METHODS.contains(&method_name) {
        return true;
    }
    let method_name = delete_punctuation(method_name);
    let variable_name = ivar_assign.name().unwrap_or("").replacen('@', "", 1);
    variable_name_candidates(cx, &method_name).contains(&variable_name)
}

fn message(cx: &Investigation<'_, '_>, var: &str, suggested_var: &str, method: &str) -> String {
    let variable_name = var.replacen('@', "", 1);
    if style(cx) == "required" && !variable_name.starts_with('_') {
        return format!(
            "Memoized variable `{var}` does not start with `_`. Use `@{suggested_var}` instead."
        );
    }
    format!(
        "Memoized variable `{var}` does not match method name `{method}`. \
         Use `@{suggested_var}` instead."
    )
}

fn suggested_var(cx: &Investigation<'_, '_>, method_name: &str) -> String {
    let suggestion = delete_punctuation(method_name);
    if style(cx) == "required" {
        format!("_{suggestion}")
    } else {
        suggestion
    }
}

fn variable_name_candidates(cx: &Investigation<'_, '_>, method_name: &str) -> Vec<String> {
    let no_underscore = method_name.strip_prefix('_').unwrap_or(method_name).to_owned();
    let with_underscore = format!("_{method_name}");
    match style(cx) {
        "required" => {
            let mut candidates = vec![with_underscore];
            if method_name.starts_with('_') {
                candidates.push(method_name.to_owned());
            }
            candidates
        }
        "disallowed" => vec![method_name.to_owned(), no_underscore],
        _ => vec![method_name.to_owned(), with_underscore, no_underscore],
    }
}
