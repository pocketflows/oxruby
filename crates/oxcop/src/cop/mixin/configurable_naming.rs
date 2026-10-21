//! Ports of `configurable_naming.rb`, `configurable_formatting.rb`,
//! `allowed_identifiers.rb`, `forbidden_identifiers.rb` and
//! `forbidden_pattern.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::mixin::ruby_regex;
use crate::regex;

/// `FORMATS` of `ConfigurableNaming`.
pub fn naming_format(style: &str) -> &'static onig::Regex {
    match style {
        "snake_case" => regex!(r"^@{0,2}[\d[[:lower:]]_]+[!?=]?$"),
        _ => regex!(r"^@{0,2}(?:_|_?[[[:lower:]]][\d[[:lower:]][[:upper:]]]*)[!?=]?$"),
    }
}

/// `valid_name?(node, name, style)` with the naming formats.
pub fn valid_name(node: Node<'_>, name: &str, format: &onig::Regex) -> bool {
    format.find(name).is_some() || class_emitter_method(node, name)
}

/// `class_emitter_method?(node, name)`.
pub fn class_emitter_method(node: Node<'_>, name: &str) -> bool {
    let Some(mut parent) = node.parent() else {
        return false;
    };
    if !node.is(NodeType::Defs) {
        return false;
    }
    let mut current = node;
    while parent.is(NodeType::Defs) {
        current = parent;
        match current.parent() {
            Some(p) => parent = p,
            None => return false,
        }
    }
    let _ = current;
    parent
        .child_nodes()
        .filter(|c| c.is(NodeType::Class))
        .any(|c| c.loc().name.is_some_and(|n| c.range_source(n) == name))
}

const SIGILS: &[char] = &['@', '$'];

/// `allowed_identifier?(name)`.
pub fn allowed_identifier(cx: &Investigation<'_, '_>, name: &str) -> bool {
    let allowed = cx.cop_config_list("AllowedIdentifiers");
    let name: String = name.chars().filter(|c| !SIGILS.contains(c)).collect();
    !allowed.is_empty() && allowed.contains(&name.as_str())
}

/// `forbidden_identifier?(name)`.
pub fn forbidden_identifier(cx: &Investigation<'_, '_>, name: &str) -> bool {
    let forbidden = cx.cop_config_list("ForbiddenIdentifiers");
    let name: String = name.chars().filter(|c| !SIGILS.contains(c)).collect();
    !forbidden.is_empty() && forbidden.contains(&name.as_str())
}

/// `forbidden_pattern?(name)`.
pub fn forbidden_pattern(cx: &Investigation<'_, '_>, name: &str) -> bool {
    cx.cop_config_list("ForbiddenPatterns")
        .iter()
        .filter_map(|p| ruby_regex(p))
        .any(|re| re.find(name).is_some())
}
