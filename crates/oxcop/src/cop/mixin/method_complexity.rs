//! Port of `lib/rubocop/cop/mixin/method_complexity.rb`: measurement and
//! reporting of complexity in methods.
//!
//! Cops handle `def`, `defs` and the block types and call
//! [`check_method_complexity`] with their complexity function.
//! `exclude_limit 'Max'` (`self.max = n`) is `set_exclude_limit`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::Investigation;
use crate::cop::metrics::utils::iterating_block::iterating_block;
use crate::cop::metrics::utils::repeated_csend_discount::RepeatedCsendDiscount;
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use serde_yaml_ng::Value;

pub const NODE_TYPES: &[NodeType] = &[
    NodeType::Def,
    NodeType::Defs,
    NodeType::Block,
    NodeType::Numblock,
    NodeType::Itblock,
];

/// `CyclomaticComplexity::COUNTED_NODES`.
pub const COUNTED_NODES: &[NodeType] = &[
    NodeType::If,
    NodeType::While,
    NodeType::Until,
    NodeType::For,
    NodeType::Csend,
    NodeType::Block,
    NodeType::BlockPass,
    NodeType::Rescue,
    NodeType::When,
    NodeType::InPattern,
    NodeType::And,
    NodeType::Or,
    NodeType::OrAsgn,
    NodeType::AndAsgn,
];

/// A complexity score and, for AbcSize, the `<A, B, C>` vector.
pub struct Complexity {
    pub score: f64,
    pub abc_vector: Option<String>,
}

/// `on_def` / `on_block`: the method name to check, unless allowed.
pub fn method_to_check<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Option<&'a str> {
    let name = if node.is_any_def() {
        node.method_name()?
    } else {
        define_method(node)?
    };
    if allowed_method(cx, name) || matches_allowed_pattern(cx, name) {
        return None;
    }
    Some(name)
}

/// `define_method?`: `(any_block (send nil? :define_method ({sym str} $_)) _ _)`.
fn define_method(node: Node<'_>) -> Option<&str> {
    if !node.is_any_block() || node.children().len() != 3 {
        return None;
    }
    let send = node.child(0).filter(|s| s.is(NodeType::Send))?;
    let [Child::Nil, Child::Sym(method), Child::Node(arg)] = send.children() else {
        return None;
    };
    if method != "define_method" {
        return None;
    }
    let arg = node.ast().node(*arg);
    if !matches!(arg.ty(), NodeType::Sym | NodeType::Str) {
        return None;
    }
    match arg.children().first() {
        Some(Child::Sym(name) | Child::Str(name)) => Some(name),
        _ => None,
    }
}

/// `cop_config['Max']`, an integer or a float.
pub fn max(cx: &Investigation<'_, '_>) -> f64 {
    match cx.cop_config_value("Max") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `check_complexity(node, method_name)`: `message(method, complexity,
/// abc_vector, max)` builds the cop's message.
pub fn check_method_complexity<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    method_name: &str,
    complexity: impl FnOnce(&Investigation<'a, '_>, Node<'a>) -> Complexity,
    message: impl FnOnce(&str, &Complexity, f64) -> String,
) {
    // Accepts empty methods always.
    let Some(body) = node.body() else {
        return;
    };
    let max = max(cx);
    let result = complexity(cx, body);
    if result.score <= max {
        return;
    }
    let msg = message(method_name, &result, max);
    let location = crate::cop::mixin::code_length::lsp_location(cx, node);
    let ceil = result.score.ceil() as i64;
    cx.add_offense_yield(location, &msg, |cx| cx.set_exclude_limit("Max", ceil));
}

/// `MethodComplexity#complexity(body)` for a cop's counted node types and
/// score function.
pub fn complexity(
    body: Node<'_>,
    counted_nodes: &[NodeType],
    score_for: fn(Node<'_>, &mut RepeatedCsendDiscount) -> usize,
) -> usize {
    let mut csend = RepeatedCsendDiscount::default();
    let mut score = 1;
    for node in body.each_node() {
        if node.is(NodeType::Lvasgn) {
            csend.reset_on_lvasgn(node);
        } else if counted_nodes.contains(&node.ty()) {
            score += score_for(node, &mut csend);
        }
    }
    score
}

/// `CyclomaticComplexity#complexity_score_for`.
pub fn complexity_score_for(node: Node<'_>, csend: &mut RepeatedCsendDiscount) -> usize {
    if iterating_block(node) == Some(false) {
        return 0;
    }
    if node.is(NodeType::Csend) && csend.discount_for_repeated_csend(node) {
        return 0;
    }
    1
}

/// `format('%.4g', value)` as Ruby prints it.
pub fn format_g4(value: f64) -> String {
    ruby_format_g(value, 4)
}

/// Ruby's `format("%.<precision>g", value)`. Ruby's `BSD__dtoa` (mode 2)
/// first tries floating-point arithmetic; when the digit after the last one
/// is within its error estimate of a half, it rounds half to even and, if
/// the exact value then says to round up, keeps the digits as they are,
/// trailing zeros included (`107.45` gives `107.4`, `131.05` gives
/// `131.0`; C's printf gives `107.5` and `131.1`).
pub fn ruby_format_g(value: f64, precision: usize) -> String {
    if value == 0.0 || !value.is_finite() {
        return if value == 0.0 {
            "0".to_owned()
        } else {
            value.to_string()
        };
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let (digits, k) = dtoa_mode2(value.abs(), precision.max(1));
    let decpt = k + 1;
    let x = decpt - 1;
    let out = if x < -4 || x >= precision as i32 {
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_owned()
        } else {
            format!("{first}.{rest}")
        };
        let exp_sign = if x < 0 { '-' } else { '+' };
        format!("{mantissa}e{exp_sign}{:02}", x.abs())
    } else if decpt <= 0 {
        format!("0.{}{digits}", "0".repeat((-decpt) as usize))
    } else {
        let int_len = decpt as usize;
        if digits.len() <= int_len {
            format!("{digits}{}", "0".repeat(int_len - digits.len()))
        } else {
            format!("{}.{}", &digits[..int_len], &digits[int_len..])
        }
    };
    format!("{sign}{out}")
}

/// `BSD__dtoa(value, 2, ndigits)` for a positive finite value: the
/// significant digits (no trailing zeros unless dtoa keeps them) and the
/// decimal exponent `k` (`value` is about `0.d1d2... * 10^(k + 1)`).
fn dtoa_mode2(value: f64, ilim: usize) -> (String, i32) {
    const TENS: [f64; 23] = [
        1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
        1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
    ];
    const BIGTENS: [f64; 5] = [1e16, 1e32, 1e64, 1e128, 1e256];
    // The decimal exponent (exact: the shortest representation never
    // crosses a power of ten).
    let shortest = format!("{value:e}");
    let k0: i32 = shortest.split_once('e').unwrap().1.parse().unwrap();

    let mut half = false;
    if ilim <= 14 {
        // Try to get by with floating-point arithmetic.
        let mut k = k0;
        let mut d = value;
        let mut ieps = 2;
        if k > 0 {
            let mut ds = TENS[(k & 0xf) as usize];
            let mut j = k >> 4;
            let mut i = 0;
            while j != 0 {
                if j & 1 != 0 {
                    ieps += 1;
                    ds *= BIGTENS[i.min(4)];
                }
                j >>= 1;
                i += 1;
            }
            d /= ds;
        } else if k < 0 {
            d *= TENS[((-k) & 0xf) as usize];
            let mut j = (-k) >> 4;
            let mut i = 0;
            while j != 0 {
                if j & 1 != 0 {
                    ieps += 1;
                    d *= BIGTENS[i.min(4)];
                }
                j >>= 1;
                i += 1;
            }
        }
        if !(0..=22).contains(&k0) && d < 1.0 {
            k -= 1;
            d *= 10.0;
            ieps += 1;
        }
        let eps = (f64::from(ieps) * d + 7.0) * f64::powi(2.0, -52) * TENS[ilim - 1];
        let mut digits: Vec<u8> = Vec::with_capacity(ilim);
        let mut ilim_q = ilim;
        let mut i = 1;
        loop {
            let l = d.trunc();
            d -= l;
            if d == 0.0 {
                ilim_q = i;
            }
            digits.push(b'0' + l as u8);
            if i == ilim_q {
                if d > 0.5 + eps {
                    return bump_up(digits, k);
                } else if d < 0.5 - eps {
                    return strip_trailing_zeros(digits, k);
                }
                half = true;
                if digits.last().unwrap() & 1 == 1 {
                    return bump_up(digits, k);
                }
                break;
            }
            i += 1;
            d *= 10.0;
        }
    }

    // The exact digits.
    let exact = format!("{value:.1100e}");
    let mantissa = exact.split_once('e').unwrap().0;
    let all: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut digits = all[..ilim.min(all.len())].to_vec();
    let rest = &all[digits.len()..];
    if rest.iter().all(|&d| d == b'0') {
        return strip_trailing_zeros(digits, k0);
    }
    let up = match rest[0] {
        b'6'..=b'9' => true,
        b'5' if rest[1..].iter().any(|&d| d != b'0') => true,
        b'5' => digits.last().unwrap() & 1 == 1,
        _ => false,
    };
    if !up {
        return strip_trailing_zeros(digits, k0);
    }
    // Round off the last digit.
    while digits.last() == Some(&b'9') {
        digits.pop();
    }
    let Some(last) = digits.last_mut() else {
        return ("1".to_owned(), k0 + 1);
    };
    if !half || *last & 1 == 1 {
        *last += 1;
    }
    (String::from_utf8(digits).unwrap(), k0)
}

fn bump_up(mut digits: Vec<u8>, k: i32) -> (String, i32) {
    while digits.last() == Some(&b'9') {
        digits.pop();
    }
    match digits.last_mut() {
        Some(last) => {
            *last += 1;
            (String::from_utf8(digits).unwrap(), k)
        }
        None => ("1".to_owned(), k + 1),
    }
}

fn strip_trailing_zeros(mut digits: Vec<u8>, k: i32) -> (String, i32) {
    while digits.len() > 1 && digits.last() == Some(&b'0') {
        digits.pop();
    }
    (String::from_utf8(digits).unwrap(), k)
}

#[cfg(test)]
mod tests {
    use super::format_g4;

    #[test]
    fn formats_like_ruby_percent_g() {
        assert_eq!(format_g4(17.0), "17");
        assert_eq!(format_g4(1.41), "1.41");
        assert_eq!(format_g4(2.24), "2.24");
        assert_eq!(format_g4(0.0), "0");
        assert_eq!(format_g4(12345.0), "1.234e+04");
        assert_eq!(format_g4(10.25), "10.25");
        assert_eq!(format_g4(100.5), "100.5");
        assert_eq!(format_g4(107.45), "107.4");
        assert_eq!(format_g4(131.05), "131.0");
        assert_eq!(format_g4(131.15), "131.2");
        assert_eq!(format_g4(2.0049), "2.005");
        assert_eq!(format_g4(107.55), "107.6");
        assert_eq!(format_g4(107.65), "107.6");
        assert_eq!(format_g4(107.447), "107.4");
        assert_eq!(format_g4(9999.5), "1e+04");
        assert_eq!(format_g4(0.000123456), "0.0001235");
        assert_eq!(format_g4(17.5), "17.5");
        assert_eq!(format_g4(1000.0), "1000");
    }
}
