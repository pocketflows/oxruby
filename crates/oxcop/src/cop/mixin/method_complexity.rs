//! Port of `lib/rubocop/cop/mixin/method_complexity.rb`: measurement and
//! reporting of complexity in methods.
//!
//! Cops handle `def`, `defs` and the block types and call
//! [`check_method_complexity`] with their complexity function.
//! `exclude_limit 'Max'` only feeds `--auto-gen-config` and is not ported;
//! `LSP.enabled?` is false.

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
    cx.add_offense(node.source_range(), &msg);
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

/// Ruby's `format('%.4g', value)`.
pub fn format_g4(value: f64) -> String {
    const PRECISION: i32 = 4;
    if value == 0.0 {
        return "0".to_owned();
    }
    let exp_form = format!("{:.*e}", (PRECISION - 1) as usize, value);
    let (mantissa, exponent) = exp_form.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if !(-4..PRECISION).contains(&exponent) {
        let mantissa = strip_zeros(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    } else {
        let decimals = (PRECISION - 1 - exponent).max(0) as usize;
        strip_zeros(&format!("{value:.decimals$}")).to_owned()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
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
    }
}
