//! Port of `lib/rubocop/cop/style/ip_addresses.rb`.

use std::sync::LazyLock;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ruby_regex;
use crate::cop::mixin::string_help::{NODE_TYPES, StringHelp};
use crate::cop::{Cop, Investigation};

/// IPv4-mapped IPv6 is the longest.
const IPV6_MAX_SIZE: usize = 45;
const MSG: &str = "Do not hardcode IP addresses.";

/// `Resolv::IPv4::Regex`.
static IPV4_REGEX: LazyLock<onig::Regex> = LazyLock::new(|| ruby_regex(r#"\A((?x-mi:0
               |1(?:[0-9][0-9]?)?
               |2(?:[0-4][0-9]?|5[0-5]?|[6-9])?
               |[3-9][0-9]?))\.((?x-mi:0
               |1(?:[0-9][0-9]?)?
               |2(?:[0-4][0-9]?|5[0-5]?|[6-9])?
               |[3-9][0-9]?))\.((?x-mi:0
               |1(?:[0-9][0-9]?)?
               |2(?:[0-4][0-9]?|5[0-5]?|[6-9])?
               |[3-9][0-9]?))\.((?x-mi:0
               |1(?:[0-9][0-9]?)?
               |2(?:[0-4][0-9]?|5[0-5]?|[6-9])?
               |[3-9][0-9]?))\z"#).unwrap());

/// `Resolv::IPv6::Regex` (an extended regexp).
static IPV6_REGEX: LazyLock<onig::Regex> =
    LazyLock::new(|| ruby_regex(concat!("(?x:", r#"
      (?:(?x-mi:\A
      (?:[0-9A-Fa-f]{1,4}:){7}
         [0-9A-Fa-f]{1,4}
      \z)) |
      (?:(?x-mi:\A
      ((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?) ::
      ((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?)
      \z)) |
      (?:(?x-mi:\A
      ((?:[0-9A-Fa-f]{1,4}:){6,6})
      (\d+)\.(\d+)\.(\d+)\.(\d+)
      \z)) |
      (?:(?x-mi:\A
      ((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?) ::
      ((?:[0-9A-Fa-f]{1,4}:)*)
      (\d+)\.(\d+)\.(\d+)\.(\d+)
      \z)) |
      (?:(?x-mi:\A
      [Ff][Ee]80
      (?::[0-9A-Fa-f]{1,4}){7}
      %[-0-9A-Za-z._~]+
      \z)) |
      (?:(?x-mi:\A
      [Ff][Ee]80:
      (?:
        ((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?) ::
        ((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?)
        |
        :((?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?)
      )?
      :[0-9A-Fa-f]{1,4}%[-0-9A-Za-z._~]+
      \z))
      "#, ")")).unwrap());

#[derive(Default)]
pub struct IpAddresses {
    string_help: StringHelp,
}

impl Cop for IpAddresses {
    fn name(&self) -> &'static str {
        "Style/IpAddresses"
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = self.string_help.candidate(node) else {
            return;
        };
        if offense(cx, node) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}

fn offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let chars: Vec<char> = node.source().chars().collect();
    if chars.len() < 2 {
        return false;
    }
    let contents: String = chars[1..chars.len() - 1].iter().collect();
    if contents.is_empty() {
        return false;
    }
    let lower = contents.to_lowercase();
    if cx
        .cop_config_list("AllowedAddresses")
        .iter()
        .any(|a| a.to_lowercase() == lower)
    {
        return false;
    }
    // To try to avoid doing two regex checks on every string, shortcut out if
    // the string does not look like an IP address.
    if !potential_ip(&contents) {
        return false;
    }
    IPV4_REGEX.find(&contents).is_some() || IPV6_REGEX.find(&contents).is_some()
}

fn potential_ip(str: &str) -> bool {
    // If the string is too long, it can't be an IP.
    if str.chars().count() > IPV6_MAX_SIZE {
        return false;
    }
    // If the string doesn't start with a colon or hexadecimal char, we know
    // it's not an IP address.
    let first_char = str.chars().next().map_or(0, |c| c as u32);
    (48..=58).contains(&first_char)
        || (65..=70).contains(&first_char)
        || (97..=102).contains(&first_char)
}
