//! Port of `lib/rubocop/cop/layout/hash_alignment.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::hash_alignment_styles::{Alignment, Deltas, is_kwsplat, value_omission};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

fn message(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Key => "Align the keys of a hash literal if they span more than one line.",
        Alignment::Separator => {
            "Align the separators of a hash literal if they span more than one line."
        }
        Alignment::Table => {
            "Align the keys and values of a hash literal if they span more than one line."
        }
        Alignment::KeywordSplat => {
            "Align keyword splats with the rest of the hash if it spans more than one line."
        }
    }
}

#[derive(Default)]
pub struct HashAlignment {
    ignored: IgnoredNodes,
}

impl Cop for HashAlignment {
    fn name(&self) -> &'static str {
        "Layout/HashAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Send, Csend, Super, Yield, Hash]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Hash) {
            self.on_hash(cx, node);
            return;
        }
        let Some(last_argument) = node.last_argument() else {
            return;
        };
        if last_argument.is(NodeType::Hash) && ignore_hash_argument(cx, last_argument) {
            self.ignored.ignore(last_argument);
        }
    }
}

/// `offenses_by` and `column_deltas`.
#[derive(Default)]
struct State<'a> {
    offenses_by: Vec<(Alignment, Vec<Node<'a>>)>,
    column_deltas: Vec<(Alignment, Vec<(Node<'a>, Deltas)>)>,
}

impl HashAlignment {
    fn on_hash<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if autocorrect_incompatible_with_other_cops(cx, node)
            || self.ignored.contains(node)
            || node.pairs().is_empty()
            || node.is_single_line()
        {
            return;
        }
        // `raise "Unknown #{key}: #{formats}"`: an error in the cop.
        let (Some(rockets), Some(colons)) = (
            new_alignment(cx, "EnforcedHashRocketStyle"),
            new_alignment(cx, "EnforcedColonStyle"),
        ) else {
            let message = format!(
                "An error occurred while {} cop was inspecting {}:{}:{}.",
                cx.cop_name(),
                cx.processed_source.path().display(),
                node.first_line(),
                node.column()
            );
            cx.errors.push(message);
            return;
        };
        if !(rockets.iter().any(|a| a.checkable_layout(node))
            && colons.iter().any(|a| a.checkable_layout(node)))
        {
            return;
        }
        check_pairs(cx, node, &rockets, &colons);
    }
}

fn autocorrect_incompatible_with_other_cops(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let enforce = cx
        .config
        .for_enabled_cop("Layout/ArgumentAlignment")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("with_fixed_indentation");
    let Some(parent) = node.parent() else {
        return false;
    };
    if !(enforce && !node.pairs().is_empty() && parent.is_call()) {
        return false;
    }
    let ps = cx.processed_source;
    let selector = match argument_before_hash(node) {
        Some(left_sibling) => left_sibling.source_range(),
        None => parent.loc().selector.unwrap_or(parent.source_range()),
    };
    let first_pair = node.pairs()[0];
    // `same_line?` (overridden): `super || node1.last_line == line(node2)`.
    ps.line_of(selector.begin_pos) == first_pair.first_line()
        || ps.line_of(selector.end_pos) == first_pair.first_line()
}

fn argument_before_hash(hash_node: Node<'_>) -> Option<Node<'_>> {
    if let Some(first) = hash_node.child(0)
        && is_kwsplat(first)
    {
        return first.child(0);
    }
    hash_node.left_sibling()
}

fn alignment_for<'v>(
    pair: Node<'_>,
    rockets: &'v [Alignment],
    colons: &'v [Alignment],
) -> &'v [Alignment] {
    if is_kwsplat(pair) {
        &[Alignment::KeywordSplat]
    } else if pair.is_hash_rocket() {
        rockets
    } else {
        colons
    }
}

fn check_pairs<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    rockets: &[Alignment],
    colons: &[Alignment],
) {
    // A child that is neither a pair nor a `kwsplat` (`**` forwarding) has no
    // `hash_rocket?`: RuboCop raises there and reports nothing for the hash.
    if node
        .child_nodes()
        .any(|c| !c.is(NodeType::Pair) && !is_kwsplat(c))
    {
        return;
    }
    let ps = cx.processed_source;
    let first_pair = node.pairs()[0];
    let mut state = State::default();

    for alignment in alignment_for(first_pair, rockets, colons) {
        let delta = alignment.deltas_for_first_pair(ps, first_pair);
        state.check_delta(delta, first_pair, *alignment);
    }
    for current in node.child_nodes() {
        for alignment in alignment_for(current, rockets, colons) {
            let delta = alignment.deltas(ps, first_pair, current);
            state.check_delta(delta, current, *alignment);
        }
    }
    add_offenses(cx, state, rockets, colons);
}

impl<'a> State<'a> {
    fn check_delta(&mut self, delta: Deltas, node: Node<'a>, alignment: Alignment) {
        let index = match self.offenses_by.iter().position(|(a, _)| *a == alignment) {
            Some(index) => index,
            None => {
                self.offenses_by.push((alignment, Vec::new()));
                self.offenses_by.len() - 1
            }
        };
        if delta.is_good() {
            return;
        }
        let deltas = match self.column_deltas.iter().position(|(a, _)| *a == alignment) {
            Some(i) => &mut self.column_deltas[i].1,
            None => {
                self.column_deltas.push((alignment, Vec::new()));
                &mut self.column_deltas.last_mut().unwrap().1
            }
        };
        // A Ruby hash keyed by nodes: structurally equal nodes share a slot.
        match deltas.iter_mut().find(|(n, _)| n.same_as(node)) {
            Some(slot) => slot.1 = delta,
            None => deltas.push((node, delta)),
        }
        self.offenses_by[index].1.push(node);
    }

    fn delta_for(&self, alignment: Alignment, node: Node<'_>) -> Option<Deltas> {
        self.column_deltas
            .iter()
            .find(|(a, _)| *a == alignment)
            .and_then(|(_, deltas)| deltas.iter().find(|(n, _)| n.same_as(node)))
            .map(|(_, d)| *d)
    }
}

fn add_offenses<'a>(
    cx: &mut Investigation<'a, '_>,
    mut state: State<'a>,
    rockets: &[Alignment],
    colons: &[Alignment],
) {
    let kwsplat_offenses = state
        .offenses_by
        .iter()
        .position(|(a, _)| *a == Alignment::KeywordSplat)
        .map(|i| state.offenses_by.remove(i).1);
    register_offenses_with_format(cx, &state, kwsplat_offenses, Alignment::KeywordSplat, rockets, colons);

    let mut min: Option<(Alignment, Vec<Node<'a>>)> = None;
    for (alignment, offenses) in &state.offenses_by {
        if min.as_ref().is_none_or(|(_, m)| offenses.len() < m.len()) {
            min = Some((*alignment, offenses.clone()));
        }
    }
    if let Some((format, offenses)) = min {
        register_offenses_with_format(cx, &state, Some(offenses), format, rockets, colons);
    }
}

fn register_offenses_with_format<'a>(
    cx: &mut Investigation<'a, '_>,
    state: &State<'a>,
    offenses: Option<Vec<Node<'a>>>,
    format: Alignment,
    rockets: &[Alignment],
    colons: &[Alignment],
) {
    let ps = cx.processed_source;
    for offense in offenses.unwrap_or_default() {
        let alignment = alignment_for(offense, rockets, colons)[0];
        let delta = state.delta_for(alignment, offense);
        cx.add_offense_with_correction(offense.source_range(), message(format), |corrector| {
            if let Some(delta) = delta {
                correct_node(ps, corrector, offense, delta);
            }
        });
    }
}

fn correct_node(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>, delta: Deltas) {
    if node.is(NodeType::Pair) && node.pair_value().is_some() && !value_omission(node) {
        correct_key_value(
            ps,
            corrector,
            delta,
            node.key().unwrap().source_range(),
            node.pair_value().unwrap().source_range(),
            node.loc().operator.unwrap(),
        );
    } else {
        let key = node.source_range();
        adjust(corrector, clamped_key_delta(ps, delta.key.unwrap_or(0), key), key);
    }
}

fn clamped_key_delta(ps: &ProcessedSource<'_>, key_delta: i64, key: Range) -> i64 {
    key_delta.max(-(ps.column_of(key.begin_pos) as i64))
}

fn correct_key_value(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    delta: Deltas,
    key: Range,
    value: Range,
    separator: Range,
) {
    let separator_delta = delta.separator.unwrap_or(0);
    let value_delta = delta.value.unwrap_or(0);
    let key_delta = delta.key.unwrap_or(0);
    adjust(corrector, clamped_key_delta(ps, key_delta, key), key);
    adjust(corrector, separator_delta, separator);
    adjust(corrector, value_delta, value);
}

fn new_alignment(cx: &Investigation<'_, '_>, key: &str) -> Option<Vec<Alignment>> {
    let mut formats = cx.cop_config_list(key);
    let mut seen = Vec::new();
    formats.retain(|f| {
        if seen.contains(f) {
            false
        } else {
            seen.push(*f);
            true
        }
    });
    formats
        .into_iter()
        .map(|format| match format {
            "key" => Some(Alignment::Key),
            "table" => Some(Alignment::Table),
            "separator" => Some(Alignment::Separator),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
}

fn adjust(corrector: &mut Corrector, delta: i64, range: Range) {
    if delta > 0 {
        corrector.insert_before(range, " ".repeat(delta as usize));
    } else if delta < 0 {
        let begin = range.begin_pos.saturating_sub(delta.unsigned_abs() as usize);
        corrector.remove(range_between(begin, range.begin_pos));
    }
}

fn ignore_hash_argument(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    match cx.cop_config_str("EnforcedLastArgumentHashStyle") {
        Some("always_ignore") => true,
        Some("ignore_explicit") => node.has_braces(),
        Some("ignore_implicit") => !node.has_braces(),
        _ => false,
    }
}
