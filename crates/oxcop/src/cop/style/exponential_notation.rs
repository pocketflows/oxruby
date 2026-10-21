//! Port of `lib/rubocop/cop/style/exponential_notation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct ExponentialNotation;

impl Cop for ExponentialNotation {
    fn name(&self) -> &'static str {
        "Style/ExponentialNotation"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Float]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let source = node.source();
        if !source.contains('e') {
            return;
        }
        let (offense, message) = match style(cx) {
            "scientific" => (!scientific(source), "Use a mantissa >= 1 and < 10."),
            "engineering" => (
                !engineering(source),
                "Use an exponent divisible by 3 and a mantissa >= 0.1 and < 1000.",
            ),
            "integral" => (
                !integral(source),
                "Use an integer as mantissa, without trailing zero.",
            ),
            _ => (false, ""),
        };
        if offense {
            cx.add_offense(node.source_range(), message);
        }
    }
}

fn parts(source: &str) -> (&str, Option<&str>) {
    let mut split = source.split('e');
    (split.next().unwrap_or(""), split.next())
}

fn scientific(source: &str) -> bool {
    regex!(r"^-?[1-9](\.\d*[0-9])?$").find(parts(source).0).is_some()
}

fn engineering(source: &str) -> bool {
    let (mantissa, exponent) = parts(source);
    let Some(exponent) = exponent.filter(|e| regex!(r"^-?\d+$").find(e).is_some()) else {
        return false;
    };
    if exponent.parse::<i64>().unwrap_or(1) % 3 != 0 {
        return false;
    }
    if regex!(r"^-?\d{4}").find(mantissa).is_some()
        || regex!(r"^-?0\d").find(mantissa).is_some()
        || regex!(r"^-?0.0").find(mantissa).is_some()
    {
        return false;
    }
    true
}

fn integral(source: &str) -> bool {
    regex!(r"^-?[1-9](\d*[1-9])?$").find(parts(source).0).is_some()
}
