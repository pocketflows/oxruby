//! Port of `lib/rubocop/cop/style/numeric_literal_prefix.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::integer_node::integer_part;
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Clone, Copy, PartialEq)]
enum LiteralType {
    OctalZeroOnly,
    Octal,
    Hex,
    Binary,
    Decimal,
}

impl LiteralType {
    fn message(self) -> &'static str {
        match self {
            LiteralType::OctalZeroOnly => "Use 0 for octal literals.",
            LiteralType::Octal => "Use 0o for octal literals.",
            LiteralType::Hex => "Use 0x for hexadecimal literals.",
            LiteralType::Binary => "Use 0b for binary literals.",
            LiteralType::Decimal => "Do not use prefixes for decimal literals.",
        }
    }

    fn format(self, source: &str) -> String {
        let re = match self {
            LiteralType::Octal => regex!(r"^0O?"),
            LiteralType::OctalZeroOnly => regex!(r"^0[Oo]?"),
            LiteralType::Hex => regex!(r"^0X"),
            LiteralType::Binary => regex!(r"^0B"),
            LiteralType::Decimal => regex!(r"^0[dD]"),
        };
        let replacement = match self {
            LiteralType::Octal => "0o",
            LiteralType::OctalZeroOnly => "0",
            LiteralType::Hex => "0x",
            LiteralType::Binary => "0b",
            LiteralType::Decimal => "",
        };
        match re.find(source) {
            Some((b, e)) => format!("{}{replacement}{}", &source[..b], &source[e..]),
            None => source.to_owned(),
        }
    }
}

#[derive(Default)]
pub struct NumericLiteralPrefix;

impl Cop for NumericLiteralPrefix {
    fn name(&self) -> &'static str {
        "Style/NumericLiteralPrefix"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Int]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(ty) = literal_type(cx, node) else {
            return;
        };
        cx.add_offense_with_correction(node.source_range(), ty.message(), |corrector| {
            let range = node.source_range();
            let range = if node.source().starts_with(['+', '-']) {
                range.adjust(1, 0)
            } else {
                range
            };
            let source = node.range_source(range);
            corrector.replace(range, ty.format(source));
        });
    }
}

fn literal_type(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<LiteralType> {
    let literal = integer_part(node);
    octal_literal_type(cx, literal).or_else(|| hex_bin_dec_literal_type(literal))
}

fn octal_literal_type(cx: &Investigation<'_, '_>, literal: &str) -> Option<LiteralType> {
    let zero_only = cx.cop_config_str("EnforcedOctalStyle") == Some("zero_only");
    if regex!(r"^0[Oo][0-7]+$").find(literal).is_some() && zero_only {
        Some(LiteralType::OctalZeroOnly)
    } else if regex!(r"^0O?[0-7]+$").find(literal).is_some() && !zero_only {
        Some(LiteralType::Octal)
    } else {
        None
    }
}

fn hex_bin_dec_literal_type(literal: &str) -> Option<LiteralType> {
    if regex!(r"^0X[0-9A-F]+$").find(literal).is_some() {
        Some(LiteralType::Hex)
    } else if regex!(r"^0B[01]+$").find(literal).is_some() {
        Some(LiteralType::Binary)
    } else if regex!(r"^0[dD][0-9]+$").find(literal).is_some() {
        Some(LiteralType::Decimal)
    } else {
        None
    }
}
