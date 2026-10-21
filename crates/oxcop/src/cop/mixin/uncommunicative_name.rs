//! Port of `lib/rubocop/cop/mixin/uncommunicative_name.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::Investigation;
use crate::processed_source::Range;
use crate::regex;

/// `check(node, args)`.
pub fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, args: &[Node<'a>]) {
    for arg in args {
        // Argument names might be "_" or prefixed with "_" to indicate they
        // are unused. Trim away this prefix and only analyse the basename.
        let full_name = match arg.children().first() {
            Some(Child::Sym(name)) => name.as_str(),
            // `nil` (anonymous arguments); destructuring (`mlhs`) children
            // are nodes, whose `to_s` never yields an offense.
            _ => continue,
        };
        if full_name == "_" {
            continue;
        }
        let name = full_name.trim_start_matches('_');
        if cx.cop_config_list("AllowedNames").contains(&name) {
            continue;
        }
        let mut length = full_name.chars().count();
        if arg.is(NodeType::Restarg) {
            length += 1;
        }
        if arg.is(NodeType::Kwrestarg) {
            length += 2;
        }
        let range = arg_range(cx, *arg, length);
        issue_offenses(cx, node, range, name);
    }
}

fn issue_offenses(cx: &mut Investigation<'_, '_>, node: Node<'_>, range: Range, name: &str) {
    let name_type = name_type(node);
    if cx.cop_config_list("ForbiddenNames").contains(&name) {
        let message = format!("Do not use {name} as a name for a {name_type}.");
        cx.add_offense(range, &message);
    }
    if regex!(r"[[:upper:]]").find(name).is_some() {
        let message = format!("Only use lowercase characters for {name_type}.");
        cx.add_offense(range, &message);
    }
    let min_length = cx.cop_config_int("MinNameLength").unwrap_or(0);
    if (name.chars().count() as i64) < min_length {
        let message = format!(
            "{} must be at least {min_length} characters long.",
            capitalize(name_type)
        );
        cx.add_offense(range, &message);
    }
    if cx.cop_config_bool("AllowNamesEndingInNumbers", false) {
        return;
    }
    if name.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        let message = format!("Do not end {name_type} with a number.");
        cx.add_offense(range, &message);
    }
}

fn name_type(node: Node<'_>) -> &'static str {
    match node.ty() {
        NodeType::Block => "block parameter",
        _ => "method parameter",
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `arg_range(arg, length)`: `length` characters from the argument's start.
fn arg_range(cx: &Investigation<'_, '_>, arg: Node<'_>, length: usize) -> Range {
    let begin_pos = arg.source_range().begin_pos;
    let source = cx.processed_source.buffer_source();
    let end_pos = source[begin_pos..]
        .char_indices()
        .nth(length)
        .map_or(source.len(), |(i, _)| begin_pos + i);
    Range::new(begin_pos, end_pos)
}
