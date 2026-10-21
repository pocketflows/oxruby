//! Port of `lib/rubocop/cop/mixin/hash_subset.rb`: shared by
//! `Style/HashExcept` and `Style/HashSlice`. The including cop supplies
//! `preferred_method_name` and `semantically_subset_method?`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::mixin::range_help::range_between;
use crate::processed_source::Range;

pub const RESTRICT_ON_SEND: &[&str] = &["reject", "select", "filter"];

const SUBSET_METHODS: &[&str] = &["==", "!=", "eql?", "include?"];
const ACTIVE_SUPPORT_SUBSET_METHODS: &[&str] = &["==", "!=", "eql?", "include?", "in?", "exclude?"];

static BLOCK_WITH_FIRST_ARG_CHECK: NodePattern = NodePattern::new(
    "(block
      (call _ _)
      (args
        $(arg _key)
        $(arg _))
      {
        $(send
          {(lvar _key) $_ _ | _ $_ (lvar _key)})
        (send
          $(send
            {(lvar _key) $_ _ | _ $_ (lvar _key)}) :!)
        })",
);

/// `on_send`/`on_csend`.
pub fn on_send(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    preferred_method_name: &str,
    semantically_subset_method: fn(Node<'_>) -> bool,
) {
    let Some((offense_range, key_source)) = extract_offense(cx, node) else {
        return;
    };
    if !semantically_subset_method(node) {
        return;
    }
    let preferred_method = format!("{preferred_method_name}({key_source})");
    let message = format!("Use `{preferred_method}` instead.");
    cx.add_offense_with_correction(offense_range, &message, |corrector| {
        corrector.replace(offense_range, &preferred_method);
    });
}

fn extract_offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<(Range, String)> {
    let block = node.parent()?;
    if !extracts_hash_subset(cx, block) {
        return None;
    }
    let except_key = except_key(block)?;
    if !safe_to_register_offense(block, except_key) {
        return None;
    }
    Some((offense_range(node), except_key_source(except_key)))
}

fn extracts_hash_subset(cx: &Investigation<'_, '_>, block: Node<'_>) -> bool {
    let Some(captures) = BLOCK_WITH_FIRST_ARG_CHECK.match_node(block) else {
        return false;
    };
    let key_arg = captures[0].node().unwrap();
    let value_arg = captures[1].node().unwrap();
    let send_node = captures[2].node().unwrap();
    let Some(method) = captures[3].sym() else {
        return false;
    };
    // Only consider methods that have one argument.
    if send_node.arguments().len() != 1 {
        return false;
    }
    if !supported_subset_method(cx, method) {
        return false;
    }
    if range_include(send_node) {
        return false;
    }
    match method {
        "include?" | "exclude?" => slices_key(send_node, false, key_arg, value_arg),
        "in?" => slices_key(send_node, true, key_arg, value_arg),
        _ => true,
    }
}

fn slices_key(send_node: Node<'_>, receiver: bool, key_arg: Node<'_>, value_arg: Node<'_>) -> bool {
    if using_value_variable(send_node, value_arg) {
        return false;
    }
    let node = if receiver {
        send_node.receiver()
    } else {
        send_node.first_argument()
    };
    node.is_some_and(|n| n.source() == key_arg.source())
}

/// When checking `include?`, `exclude?` and `in?` for offenses, if the
/// receiver or first argument is a range, an offense should not be
/// registered (`(1..5).include?(k)` or `k.in?('a'..'z')`).
fn range_include(send_node: Node<'_>) -> bool {
    if send_node.first_argument().is_some_and(|a| a.is_range()) {
        return true;
    }
    let mut receiver = send_node.receiver();
    while let Some(r) = receiver.filter(|r| r.is(NodeType::Begin)) {
        receiver = r.child_nodes().next();
    }
    receiver.is_some_and(|r| r.is_range())
}

/// If the receiver of `include?` or `exclude?`, or the first argument of
/// `in?` is the hash value block argument, an offense should not be
/// registered (`v.include?(k)` or `k.in?(v)`).
fn using_value_variable(send_node: Node<'_>, value_arg: Node<'_>) -> bool {
    let is_value = |n: Option<Node<'_>>| {
        n.is_some_and(|n| n.is(NodeType::Lvar) && n.name() == value_arg.name())
    };
    is_value(send_node.receiver()) || is_value(send_node.first_argument())
}

fn supported_subset_method(cx: &Investigation<'_, '_>, method: &str) -> bool {
    if cx
        .config
        .all_cops_bool("ActiveSupportExtensionsEnabled", false)
    {
        ACTIVE_SUPPORT_SUBSET_METHODS.contains(&method)
    } else {
        SUBSET_METHODS.contains(&method)
    }
}

pub fn semantically_except_method(node: Node<'_>) -> bool {
    let Some(body) = node.parent().and_then(|b| b.body()) else {
        return false;
    };
    let (body, negated) = extract_body_if_negated(body);
    if node.is_method("reject") {
        body.is_method("==") || body.is_method("eql?") || included(body, negated)
    } else {
        body.is_method("!=") || not_included(body, negated)
    }
}

pub fn semantically_slice_method(node: Node<'_>) -> bool {
    !semantically_except_method(node)
}

fn included(body: Node<'_>, negated: bool) -> bool {
    if negated {
        body.is_method("exclude?")
    } else {
        body.is_method("include?") || body.is_method("in?")
    }
}

fn not_included(body: Node<'_>, negated: bool) -> bool {
    included(body, !negated)
}

fn safe_to_register_offense(block: Node<'_>, except_key: Node<'_>) -> bool {
    let Some(body) = block.body() else {
        return false;
    };
    if body.is_method("==") || body.is_method("!=") {
        except_key.is_any(&[NodeType::Sym, NodeType::Str])
    } else {
        true
    }
}

fn extract_body_if_negated(body: Node<'_>) -> (Node<'_>, bool) {
    if body.is_method("!") {
        (body.receiver().unwrap(), true)
    } else {
        (body, false)
    }
}

fn except_key_source(key: Node<'_>) -> String {
    if key.is(NodeType::Array) {
        let values: Vec<String> = if key.is_percent_literal() {
            key.child_nodes().map(decorate_source).collect()
        } else {
            key.child_nodes().map(|v| v.source().to_owned()).collect()
        };
        return values.join(", ");
    }
    if key.is_literal() {
        key.source().to_owned()
    } else {
        format!("*{}", key.source())
    }
}

fn decorate_source(value: Node<'_>) -> String {
    match value.ty() {
        NodeType::Dsym => format!(":\"{}\"", value.source()),
        NodeType::Dstr => format!("\"{}\"", value.source()),
        NodeType::Sym => format!(":{}", value.source()),
        // The element of a `%w` array can contain characters that are special
        // inside a single-quoted string (e.g. a `'`), so escape them rather
        // than wrapping the raw source.
        NodeType::Str => to_single_quoted(value.str_content().unwrap_or("")),
        _ => format!("'{}'", value.source()),
    }
}

fn to_single_quoted(string: &str) -> String {
    let mut out = String::from("'");
    for c in string.chars() {
        if c == '\'' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn except_key(node: Node<'_>) -> Option<Node<'_>> {
    let key_arg = node
        .child(1)?
        .descendants()
        .find(|d| d.is_argument_type())?
        .source();
    let (body, _) = extract_body_if_negated(node.body()?);
    let lhs = body.child(0);
    let rhs = body.child(2);
    if lhs.is_some_and(|l| l.source() == key_arg) {
        rhs
    } else {
        lhs
    }
}

fn offense_range(node: Node<'_>) -> Range {
    range_between(
        node.loc().selector.unwrap().begin_pos,
        node.parent().unwrap().loc().end.unwrap().end_pos,
    )
}
