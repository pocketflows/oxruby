//! Port of `lib/rubocop/cop/style/hash_syntax.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::{correct_style_detected, style, style_detected};
use crate::cop::mixin::hash_shorthand_syntax::{on_hash_for_mixed_shorthand, on_pair};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_with_surrounding_space,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::regex;

const MSG_19: &str = "Use the new Ruby 1.9 hash syntax.";
const MSG_NO_MIXED_KEYS: &str = "Don't mix styles in the same hash.";
const MSG_HASH_ROCKETS: &str = "Use hash rockets syntax.";

#[derive(Default)]
pub struct HashSyntax;

impl Cop for HashSyntax {
    fn name(&self) -> &'static str {
        "Style/HashSyntax"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash, NodeType::Pair]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Pair) {
            on_pair(cx, node, hash_rockets_enforced);
            return;
        }
        let pairs = node.pairs();
        if pairs.is_empty() {
            return;
        }
        on_hash_for_mixed_shorthand(cx, node, hash_rockets_enforced);
        let style = style(cx);
        if style == "hash_rockets" || force_hash_rockets(cx, &pairs) {
            check(cx, &pairs, ":", MSG_HASH_ROCKETS);
        } else if style == "ruby19_no_mixed_keys" {
            if sym_indices(cx, &pairs) {
                check(cx, &pairs, "=>", MSG_19);
            } else {
                check(cx, &pairs, ":", MSG_NO_MIXED_KEYS);
            }
        } else if style == "no_mixed_keys" {
            if sym_indices(cx, &pairs) {
                check(cx, &pairs, inverse_delimiter(pairs[0], false), MSG_NO_MIXED_KEYS);
            } else {
                check(cx, &pairs, ":", MSG_NO_MIXED_KEYS);
            }
        } else if sym_indices(cx, &pairs) {
            check(cx, &pairs, "=>", MSG_19);
        }
    }
}

fn hash_rockets_enforced(cx: &Investigation<'_, '_>, hash_node: Node<'_>) -> bool {
    style(cx) == "hash_rockets" || force_hash_rockets(cx, &hash_node.pairs())
}

/// `PairNode#delimiter`.
fn delimiter(pair: Node<'_>, with_spacing: bool) -> &'static str {
    match (pair.is_hash_rocket(), with_spacing) {
        (true, false) => "=>",
        (true, true) => " => ",
        (false, false) => ":",
        (false, true) => ": ",
    }
}

/// `PairNode#inverse_delimiter`.
fn inverse_delimiter(pair: Node<'_>, with_spacing: bool) -> &'static str {
    match (pair.is_hash_rocket(), with_spacing) {
        (true, false) => ":",
        (true, true) => ": ",
        (false, false) => "=>",
        (false, true) => " => ",
    }
}

fn autocorrect(cx: &Investigation<'_, '_>, corrector: &mut Corrector, node: Node<'_>) {
    let style = style(cx);
    let parent_pairs = node.parent().map(|p| p.pairs()).unwrap_or_default();
    if style == "hash_rockets" || force_hash_rockets(cx, &parent_pairs) {
        autocorrect_hash_rockets(cx, corrector, node);
    } else if matches!(style, "ruby19_no_mixed_keys" | "no_mixed_keys") {
        if node.is_colon() {
            autocorrect_hash_rockets(cx, corrector, node);
        } else {
            autocorrect_ruby19(cx, corrector, node);
        }
    } else {
        autocorrect_ruby19(cx, corrector, node);
    }
}

fn sym_indices(cx: &Investigation<'_, '_>, pairs: &[Node<'_>]) -> bool {
    pairs.iter().all(|p| word_symbol_pair(cx, *p))
}

fn word_symbol_pair(cx: &Investigation<'_, '_>, pair: Node<'_>) -> bool {
    let Some(key) = pair.key().filter(|k| k.is_any_sym()) else {
        return false;
    };
    acceptable_19_syntax_symbol(cx, key.source())
}

fn acceptable_19_syntax_symbol(cx: &Investigation<'_, '_>, sym_name: &str) -> bool {
    let sym_name = sym_name.strip_prefix(':').unwrap_or(sym_name);
    // Prefer { :production? => false } over { production?: false } and
    // similarly for other non-alnum final characters (except quotes).
    if cx.cop_config_bool("PreferHashRocketsForNonAlnumEndingSymbols", false)
        && regex!(r#"[\p{Alnum}"']\z"#).find(sym_name).is_none()
    {
        return false;
    }
    // Most hash keys can be matched against a simple regex.
    if regex!(r"(?i)\A[_a-z]\w*[?!]?\z").find(sym_name).is_some() {
        return true;
    }
    if cx.target_ruby_version() <= 2.1 {
        return false;
    }
    (sym_name.starts_with('\'') && sym_name.ends_with('\''))
        || (sym_name.starts_with('"') && sym_name.ends_with('"'))
}

fn check(cx: &mut Investigation<'_, '_>, pairs: &[Node<'_>], delim: &str, msg: &str) {
    for &pair in pairs {
        if delimiter(pair, false) != delim {
            correct_style_detected(cx, "EnforcedStyle");
            continue;
        }
        let location = pair
            .source_range()
            .begin()
            .join(pair.loc().operator.unwrap());
        cx.add_offense_cx(location, msg, |corrector, cx| {
            autocorrect(cx, corrector, pair);
        });
        // `opposite_style_detected`, in the correction block (which runs
        // when the offense's line is enabled).
        if cx.enabled_line(cx.processed_source.line_of(location.begin_pos)) {
            let alternative = alternative_style(cx);
            style_detected(cx, "EnforcedStyle", alternative.as_slice());
        }
    }
}

/// `alternative_style` (overridden): `nil` for the no-mixed-keys styles.
fn alternative_style(cx: &Investigation<'_, '_>) -> Vec<&'static str> {
    match style(cx) {
        "hash_rockets" => vec!["ruby19"],
        "ruby19" | "ruby19_no_mixed_keys" => vec!["hash_rockets"],
        _ => Vec::new(),
    }
}

fn autocorrect_ruby19(cx: &Investigation<'_, '_>, corrector: &mut Corrector, pair_node: Node<'_>) {
    let src = cx.processed_source.buffer_source();
    let key = pair_node.key().unwrap().source_range();
    let operator = pair_node.loc().operator.unwrap();
    let range = range_with_surrounding_space(src, key.join(operator), SurroundingSpace::side(Side::Right));

    let hash_node = pair_node.parent().unwrap();
    let space = if argument_without_space(hash_node) {
        " "
    } else {
        ""
    };
    let source = cx.source(range);
    let replaced = match regex!(r"^:(.*\S)\s*=>\s*$").captures(source) {
        Some(caps) => {
            let (b, e) = caps.pos(0).unwrap();
            format!(
                "{}{space}{}: {}",
                &source[..b],
                caps.at(1).unwrap_or(""),
                &source[e..]
            )
        }
        None => source.to_owned(),
    };
    corrector.replace(range, replaced);

    if !(hash_node.parent().is_some_and(|p| p.is(NodeType::Return)) && !hash_node.has_braces()) {
        return;
    }
    // This runs once per pair, but the hash must only be wrapped once.
    if hash_node.pairs().first() != Some(&pair_node) {
        return;
    }
    corrector.wrap(hash_node.source_range(), "{", "}");
}

fn argument_without_space(node: Node<'_>) -> bool {
    if !node.is_argument() {
        return false;
    }
    let Some(selector) = node.parent().and_then(|p| p.loc().selector) else {
        return false;
    };
    node.source_range().begin_pos == selector.end_pos
}

fn autocorrect_hash_rockets(cx: &Investigation<'_, '_>, corrector: &mut Corrector, pair_node: Node<'_>) {
    let src = cx.processed_source.buffer_source();
    let op = pair_node.loc().operator.unwrap();
    let key = pair_node.key().unwrap();
    let mut key_with_hash_rocket = format!(":{}{}", key.source(), inverse_delimiter(pair_node, true));
    if pair_node.is_value_omission() {
        key_with_hash_rocket.push_str(key.source());
    }
    corrector.replace(key.source_range(), key_with_hash_rocket);
    corrector.remove(range_with_surrounding_space(src, op, SurroundingSpace::default()));
}

fn force_hash_rockets(cx: &Investigation<'_, '_>, pairs: &[Node<'_>]) -> bool {
    cx.cop_config_bool("UseHashRocketsWithSymbolValues", false)
        && pairs
            .iter()
            .filter_map(|p| p.pair_value())
            .any(|v| v.is(NodeType::Sym))
}
