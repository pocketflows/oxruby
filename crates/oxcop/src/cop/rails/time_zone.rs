//! Port of rubocop-rails' `lib/rubocop/cop/rails/time_zone.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Corrector, Cop, Investigation};

const MSG: &str = "Do not use `%<current>s` without zone. Use `%<prefer>s` instead.";
const MSG_ACCEPTABLE: &str =
    "Do not use `%<current>s` without zone. Use one of %<prefer>s instead.";
const MSG_LOCALTIME: &str = "Do not use `Time.localtime` without offset or zone.";
const MSG_STRING_TO_TIME: &str =
    "Do not use `String#to_time` without zone. Use `Time.zone.parse` instead.";

const GOOD_METHODS: &[&str] = &["zone", "zone_default", "find_zone", "find_zone!"];
const DANGEROUS_METHODS: &[&str] = &["now", "local", "new", "parse", "at"];
const ACCEPTED_METHODS: &[&str] = &[
    "in_time_zone",
    "utc",
    "getlocal",
    "xmlschema",
    "iso8601",
    "jisx0301",
    "rfc3339",
    "httpdate",
    "to_i",
    "to_f",
];

#[derive(Default)]
pub struct TimeZone;

impl Cop for TimeZone {
    fn name(&self) -> &'static str {
        "Rails/TimeZone"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const, NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["to_time"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Const) {
            on_const(cx, node);
        } else {
            on_send(cx, node);
        }
    }
}

fn on_const<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    // we should only check core classes
    // (`Time` or `::Time`)
    let top_level = match node.child_value(0) {
        None | Some(Child::Nil) => true,
        Some(_) => node.child(0).is_some_and(|m| m.is(NodeType::Cbase)),
    };
    if !(top_level && method_send(node)) {
        return;
    }
    if node.child_sym(1) == Some("Time")
        && let Some(parent) = node.parent()
    {
        check_time_node(cx, "Time", parent);
    }
}

// `on_send`, aliased as `on_csend`.
fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(receiver) = node.receiver().filter(|r| r.is(NodeType::Str)) else {
        return;
    };
    if !node.is_method("to_time") {
        return;
    }
    if attach_timezone_specifier(Some(receiver)) {
        return;
    }
    let Some(selector) = node.loc().selector else {
        return;
    };
    let csend = node.is(NodeType::Csend);
    let range = node.source_range();
    let replacement = format!("Time.zone.parse({})", receiver.source());
    cx.add_offense_with_correction(selector, MSG_STRING_TO_TIME, |corrector| {
        if !csend {
            corrector.replace(range, replacement);
        }
    });
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, strict: bool) {
    let Some(receiver) = node.child(0) else {
        return;
    };
    // add `.zone`: `Time.at` => `Time.zone.at`
    corrector.insert_after(receiver.source_range(), ".zone");

    match node.method_name() {
        // replace `Time.zone.current` => `Time.zone.now`
        Some("current") => {
            if let Some(selector) = node.loc().selector {
                corrector.replace(selector, "now");
            }
        }
        Some("new") => autocorrect_time_new(node, corrector),
        _ => {}
    }

    // prefer `Time` over `DateTime` class
    if strict {
        corrector.replace(receiver.source_range(), "Time");
    }
    remove_redundant_in_time_zone(corrector, node);
}

fn autocorrect_time_new(node: Node<'_>, corrector: &mut Corrector) {
    if let Some(selector) = node.loc().selector {
        corrector.replace(selector, replacement(node));
    }
}

// remove redundant `.in_time_zone` from `Time.zone.now.in_time_zone`
fn remove_redundant_in_time_zone(corrector: &mut Corrector, node: Node<'_>) {
    let time_methods_called = extract_method_chain(node);
    if !time_methods_called.contains(&"in_time_zone") && !time_methods_called.contains(&"zone") {
        return;
    }
    let mut current = Some(node);
    while let Some(n) = current.filter(|n| n.is(NodeType::Send)) {
        if n.children().last().and_then(Child::as_sym) == Some("in_time_zone")
            && let Some(selector) = n.loc().selector
        {
            corrector.remove(selector.adjust(-1, 0));
        }
        current = n.parent();
    }
}

fn check_time_node<'a>(cx: &mut Investigation<'a, '_>, klass: &str, node: Node<'a>) {
    if attach_timezone_specifier(node.first_argument()) {
        return;
    }

    let chain = extract_method_chain(node);
    if not_danger_chain(cx, &chain) {
        return;
    }
    if need_check_localtime(cx, &chain) {
        check_localtime(cx, node);
        return;
    }

    let method_name = intersection(&chain, DANGEROUS_METHODS).join(".");

    if offset_provided(node) {
        return;
    }

    let message = build_message(cx, klass, &method_name, node);
    let Some(selector) = node.loc().selector else {
        return;
    };
    let strict = is_strict(cx);
    cx.add_offense_with_correction(selector, &message, |corrector| {
        autocorrect(corrector, node, strict);
    });
}

/// `attach_timezone_specifier?`: `date.respond_to?(:value)` holds for the
/// literal node classes (`StrNode`, `DstrNode`, `SymbolNode`, the numeric
/// nodes; a `pair` can't be an argument).
fn attach_timezone_specifier(date: Option<Node<'_>>) -> bool {
    let Some(date) = date else {
        return false;
    };
    match literal_value(date) {
        // `!date.value.to_s.valid_encoding?`
        Some(None) => true,
        Some(Some(value)) => timezone_specifier(&value),
        None => false,
    }
}

/// `node.value.to_s` for a node that responds to `value`: `None` when it
/// doesn't, `Some(None)` when the string isn't valid in its encoding.
fn literal_value(node: Node<'_>) -> Option<Option<String>> {
    match node.ty() {
        NodeType::Str | NodeType::Sym | NodeType::Int | NodeType::Float
        | NodeType::Rational | NodeType::Complex => Some(child_to_s(node.children().first())),
        // `xstr` is a `StrNode` whose value is its first part (a node, whose
        // `to_s` is an S-expression ending in `)`), or `nil`.
        NodeType::Xstr => Some(Some(if node.children().is_empty() {
            String::new()
        } else {
            ")".to_owned()
        })),
        NodeType::Dstr => {
            let mut value = String::new();
            for child in node.child_nodes() {
                match literal_value(child) {
                    Some(Some(part)) => value.push_str(&part),
                    Some(None) => return Some(None),
                    None => value.push_str(child.source()),
                }
            }
            Some(Some(value))
        }
        _ => None,
    }
}

/// `value.to_s` of a literal's first child.
fn child_to_s(child: Option<&Child>) -> Option<String> {
    Some(match child {
        None | Some(Child::Nil) => String::new(),
        Some(Child::Str(s) | Child::Sym(s) | Child::BigInt(s)) => s.clone(),
        Some(Child::Bytes(_)) => return None,
        Some(Child::Int(i)) => i.to_string(),
        // Never ends in a letter or a `[+-]dd:dd` offset.
        Some(Child::Float(_) | Child::Rational(..) | Child::Node(_)) => String::new(),
        // `Complex#to_s` ends in `i`.
        Some(Child::Complex(_)) => "i".to_owned(),
    })
}

/// `TIMEZONE_SPECIFIER = /([A-Za-z]|[+-]\d{2}:?\d{2})\z/`.
fn timezone_specifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(&last) = bytes.last() else {
        return false;
    };
    if last.is_ascii_alphabetic() {
        return true;
    }
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let n = bytes.len();
    if n < 5 || !digit(n - 1) || !digit(n - 2) {
        return false;
    }
    let mut i = n - 3;
    if bytes[i] == b':' {
        if i < 3 {
            return false;
        }
        i -= 1;
    }
    digit(i) && digit(i - 1) && matches!(bytes[i - 2], b'+' | b'-')
}

fn build_message(cx: &Investigation<'_, '_>, klass: &str, method_name: &str, node: Node<'_>) -> String {
    let current = format!("{klass}.{method_name}");
    if is_flexible(cx) {
        MSG_ACCEPTABLE
            .replace("%<current>s", &current)
            .replace(
                "%<prefer>s",
                &acceptable_methods(klass, method_name, node).join(", "),
            )
    } else {
        let safe_method_name = safe_method(method_name, node);
        MSG.replace("%<current>s", &current)
            .replace("%<prefer>s", &format!("Time.zone.{safe_method_name}"))
    }
}

fn extract_method_chain<'a>(node: Node<'a>) -> Vec<&'a str> {
    let mut chain = Vec::new();
    let mut current = Some(node);
    while let Some(n) = current.filter(|n| n.is(NodeType::Send)) {
        if method_from_time_class(n)
            && let Some(method_name) = n.method_name()
        {
            chain.push(method_name);
        }
        current = n.parent();
    }
    chain
}

// Only add the method to the chain if the method being
// called is part of the time class.
fn method_from_time_class(node: Node<'_>) -> bool {
    match node.child(0) {
        Some(receiver) if !receiver.is(NodeType::Cbase) => method_from_time_class(receiver),
        _ => node.child_sym(1) == Some("Time"),
    }
}

// checks that parent node of send_type
// and receiver is the given node
fn method_send(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| parent.is(NodeType::Send) && parent.receiver() == Some(node))
}

fn safe_method<'m>(method_name: &'m str, node: Node<'_>) -> &'m str {
    if matches!(method_name, "new" | "current") {
        replacement(node)
    } else {
        method_name
    }
}

fn check_localtime<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let selector_node = node;
    let mut current = Some(node);
    while let Some(n) = current.filter(|n| n.is(NodeType::Send)) {
        if n.is_method("localtime") {
            break;
        }
        current = n.parent();
    }

    if current.is_some_and(|n| n.has_arguments()) {
        return;
    }

    let Some(selector) = selector_node.loc().selector else {
        return;
    };
    let strict = is_strict(cx);
    cx.add_offense_with_correction(selector, MSG_LOCALTIME, |corrector| {
        autocorrect(corrector, selector_node, strict);
    });
}

fn not_danger_chain(cx: &Investigation<'_, '_>, chain: &[&str]) -> bool {
    intersection(chain, DANGEROUS_METHODS).is_empty()
        || chain.iter().any(|m| is_good_method(cx, m))
}

fn need_check_localtime(cx: &Investigation<'_, '_>, chain: &[&str]) -> bool {
    is_flexible(cx) && chain.contains(&"localtime")
}

fn is_flexible(cx: &Investigation<'_, '_>) -> bool {
    style(cx) == "flexible"
}

fn is_strict(cx: &Investigation<'_, '_>) -> bool {
    style(cx) == "strict"
}

/// `good_methods.include?(method)`.
fn is_good_method(cx: &Investigation<'_, '_>, method: &str) -> bool {
    GOOD_METHODS.contains(&method)
        || (!is_strict(cx) && (method == "current" || ACCEPTED_METHODS.contains(&method)))
}

fn acceptable_methods(klass: &str, method_name: &str, node: Node<'_>) -> Vec<String> {
    let mut acceptable = vec![
        format!("`Time.zone.{}`", safe_method(method_name, node)),
        format!("`{klass}.current`"),
    ];
    for am in ACCEPTED_METHODS {
        acceptable.push(format!("`{klass}.{method_name}.{am}`"));
    }
    acceptable
}

// Time.new, Time.at, and Time.now can be called with a time zone offset
// When it is, that should be considered safe
// Example:
// Time.new(1988, 3, 15, 3, 0, 0, "-05:00")
fn offset_provided(node: Node<'_>) -> bool {
    match node.method_name() {
        Some("new") => node.arguments().len() == 7 || offset_option_provided(node),
        Some("at" | "now") => offset_option_provided(node),
        _ => false,
    }
}

fn offset_option_provided(node: Node<'_>) -> bool {
    node.last_argument().is_some_and(|options| {
        options.is(NodeType::Hash)
            && options.pairs().iter().any(|pair| {
                pair.key()
                    .is_some_and(|key| key.sym_value() == Some("in"))
                    && pair.pair_value().is_some_and(|value| !value.is(NodeType::Nil))
            })
    })
}

fn replacement(node: Node<'_>) -> &'static str {
    match node.first_argument() {
        None => "now",
        Some(first) if first.is(NodeType::Str) => "parse",
        Some(_) => "local",
    }
}

/// `chain & other`: the elements of `chain` that are in `other`, in order,
/// without duplicates.
fn intersection<'a>(chain: &[&'a str], other: &[&str]) -> Vec<&'a str> {
    let mut result: Vec<&'a str> = Vec::new();
    for method in chain {
        if other.contains(method) && !result.contains(method) {
            result.push(method);
        }
    }
    result
}
