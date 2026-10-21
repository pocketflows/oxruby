//! Port of `lib/rubocop/cop/mixin/hash_alignment_styles.rb`, plus the
//! rubocop-ast `HashElementNode` / `PairNode` helpers it relies on
//! (`key_delta`, `value_delta`, `delimiter_delta`, `delimiter`, ...).

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::begins_its_line;
use crate::processed_source::ProcessedSource;

/// A deltas hash (`{ key:, separator:, value: }`); absent keys are `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deltas {
    pub key: Option<i64>,
    pub separator: Option<i64>,
    pub value: Option<i64>,
}

impl Deltas {
    /// `good_alignment?`: every value is zero.
    pub fn is_good(&self) -> bool {
        [self.key, self.separator, self.value]
            .iter()
            .all(|d| d.is_none_or(|d| d == 0))
    }
}

/// The alignment classes (`KeyAlignment`, `TableAlignment`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Key,
    Table,
    Separator,
    KeywordSplat,
}

impl Alignment {
    pub fn checkable_layout(self, node: Node<'_>) -> bool {
        match self {
            Alignment::Key | Alignment::KeywordSplat => true,
            Alignment::Table | Alignment::Separator => {
                !pairs_on_same_line(node) && !mixed_delimiters(node) && alignable_values(node)
            }
        }
    }

    pub fn deltas_for_first_pair(self, ps: &ProcessedSource<'_>, first_pair: Node<'_>) -> Deltas {
        match self {
            Alignment::Key => Deltas {
                key: None,
                separator: Some(key_separator_delta(ps, first_pair)),
                value: Some(key_value_delta(ps, first_pair)),
            },
            Alignment::Table => {
                let separator_delta = value_separator_delta(self, ps, first_pair, first_pair, 0);
                Deltas {
                    key: None,
                    separator: Some(separator_delta),
                    value: Some(table_value_delta(ps, first_pair, first_pair) - separator_delta),
                }
            }
            Alignment::Separator | Alignment::KeywordSplat => Deltas::default(),
        }
    }

    pub fn deltas(
        self,
        ps: &ProcessedSource<'_>,
        first_pair: Node<'_>,
        current_pair: Node<'_>,
    ) -> Deltas {
        match self {
            Alignment::Key => {
                if begins_its_line(ps, current_pair.source_range()) {
                    Deltas {
                        key: Some(key_delta(ps, first_pair, current_pair, Side::Left)),
                        separator: Some(key_separator_delta(ps, current_pair)),
                        value: Some(key_value_delta(ps, current_pair)),
                    }
                } else {
                    Deltas::default()
                }
            }
            Alignment::Table | Alignment::Separator => {
                let key_delta = match self {
                    Alignment::Table => key_delta(ps, first_pair, current_pair, Side::Left),
                    _ => key_delta(ps, first_pair, current_pair, Side::Right),
                };
                let separator_delta =
                    value_separator_delta(self, ps, first_pair, current_pair, key_delta);
                let value_delta = match self {
                    Alignment::Table => table_value_delta(ps, first_pair, current_pair),
                    _ => separator_value_delta(ps, first_pair, current_pair),
                } - key_delta
                    - separator_delta;
                Deltas {
                    key: Some(key_delta),
                    separator: Some(separator_delta),
                    value: Some(value_delta),
                }
            }
            Alignment::KeywordSplat => {
                if begins_its_line(ps, current_pair.source_range()) {
                    Deltas {
                        key: Some(key_delta(ps, first_pair, current_pair, Side::Left)),
                        ..Deltas::default()
                    }
                } else {
                    Deltas::default()
                }
            }
        }
    }
}

// ---- KeyAlignment ------------------------------------------------------------

fn key_separator_delta(ps: &ProcessedSource<'_>, pair: Node<'_>) -> i64 {
    if pair.is_hash_rocket() {
        let correct_separator_column = col(ps, pair.key().unwrap().source_range().end_pos) + 1;
        let actual_separator_column = col(ps, pair.loc().operator.unwrap().begin_pos);
        correct_separator_column - actual_separator_column
    } else {
        0
    }
}

fn key_value_delta(ps: &ProcessedSource<'_>, pair: Node<'_>) -> i64 {
    if value_on_new_line(pair) || value_omission(pair) {
        return 0;
    }
    let correct_value_column = col(ps, pair.loc().operator.unwrap().end_pos) + 1;
    let actual_value_column = col(ps, pair.pair_value().unwrap().source_range().begin_pos);
    correct_value_column - actual_value_column
}

// ---- ValueAlignment ----------------------------------------------------------

fn alignable_values(node: Node<'_>) -> bool {
    let pairs = node.pairs();
    if pairs.first().is_some_and(|p| value_omission(*p)) {
        return false;
    }
    !pairs.iter().any(|pair| value_on_later_line(*pair))
}

fn value_on_later_line(pair: Node<'_>) -> bool {
    pair.pair_value()
        .is_some_and(|value| pair.key().unwrap().last_line() != value.first_line())
}

fn value_separator_delta(
    alignment: Alignment,
    ps: &ProcessedSource<'_>,
    first_pair: Node<'_>,
    current_pair: Node<'_>,
    key_delta: i64,
) -> i64 {
    if !current_pair.is_hash_rocket() {
        return 0;
    }
    let hash_rocket_delta = match alignment {
        Alignment::Table => {
            target_operator_column(ps, first_pair)
                - col(ps, current_pair.loc().operator.unwrap().begin_pos)
        }
        _ => delimiter_delta(ps, first_pair, current_pair),
    };
    hash_rocket_delta - key_delta
}

// ---- TableAlignment ----------------------------------------------------------

fn table_value_delta(
    ps: &ProcessedSource<'_>,
    first_pair: Node<'_>,
    current_pair: Node<'_>,
) -> i64 {
    let correct_value_column = target_operator_column(ps, first_pair)
        + max_delimiter_width(first_pair.parent().unwrap())
        - 1;
    if value_omission(current_pair) {
        0
    } else {
        correct_value_column
            - col(
                ps,
                current_pair.pair_value().unwrap().source_range().begin_pos,
            )
    }
}

fn target_operator_column(ps: &ProcessedSource<'_>, first_pair: Node<'_>) -> i64 {
    let hash_node = first_pair.parent().unwrap();
    let mut candidates: Vec<i64> = hash_node
        .hash_keys()
        .into_iter()
        .filter(|key| !key.is_single_line())
        .map(|key| col(ps, key.source_range().end_pos) + 1)
        .collect();
    let key_width = hash_node
        .hash_keys()
        .into_iter()
        .filter(|key| key.is_single_line())
        .map(|key| key.source().chars().count() as i64)
        .max()
        .unwrap_or(0);
    if key_width > 0 {
        candidates.push(col(ps, first_pair.source_range().begin_pos) + key_width + 1);
    }
    candidates.into_iter().max().unwrap_or(0)
}

fn max_delimiter_width(hash_node: Node<'_>) -> i64 {
    hash_node
        .pairs()
        .iter()
        .map(|pair| delimiter(*pair, true).len() as i64)
        .max()
        .unwrap_or(0)
}

// ---- SeparatorAlignment ------------------------------------------------------

fn separator_value_delta(
    ps: &ProcessedSource<'_>,
    first_pair: Node<'_>,
    current_pair: Node<'_>,
) -> i64 {
    if value_omission(current_pair) {
        0
    } else {
        value_delta(ps, first_pair, current_pair)
    }
}

// ---- rubocop-ast HashElementNode / PairNode / HashNode ------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

fn col(ps: &ProcessedSource<'_>, pos: usize) -> i64 {
    ps.column_of(pos) as i64
}

/// `HashElementNode#same_line?`.
pub fn element_same_line(first: Node<'_>, other: Node<'_>) -> bool {
    first.last_line() == other.first_line() || first.first_line() == other.last_line()
}

/// `kwsplat_type?`: rubocop-ast makes `forwarded_kwrestarg` (`**` in a
/// call) a `KeywordSplatNode` too.
pub fn is_kwsplat(node: Node<'_>) -> bool {
    node.is(NodeType::Kwsplat) || node.is(NodeType::ForwardedKwrestarg)
}

/// `node_parts[0]`: a pair's key; a keyword splat is its own key.
fn element_key(node: Node<'_>) -> Node<'_> {
    if is_kwsplat(node) {
        node
    } else {
        node.key().unwrap()
    }
}

fn keyword_splat(first: Node<'_>, second: Node<'_>) -> bool {
    is_kwsplat(first) || is_kwsplat(second)
}

/// `HashElementNode#key_delta(other, alignment)`.
pub fn key_delta(
    ps: &ProcessedSource<'_>,
    first: Node<'_>,
    second: Node<'_>,
    alignment: Side,
) -> i64 {
    if element_same_line(first, second) {
        return 0;
    }
    if keyword_splat(first, second) && alignment == Side::Right {
        return 0;
    }
    let (a, b) = (
        element_key(first).source_range(),
        element_key(second).source_range(),
    );
    match alignment {
        Side::Left => col(ps, a.begin_pos) - col(ps, b.begin_pos),
        Side::Right => col(ps, a.end_pos) - col(ps, b.end_pos),
    }
}

/// `HashElementNode#value_delta(other)`.
pub fn value_delta(ps: &ProcessedSource<'_>, first: Node<'_>, second: Node<'_>) -> i64 {
    if element_same_line(first, second) || keyword_splat(first, second) {
        return 0;
    }
    col(ps, first.pair_value().unwrap().source_range().begin_pos)
        - col(ps, second.pair_value().unwrap().source_range().begin_pos)
}

/// `HashElementNode#delimiter_delta(other)`.
pub fn delimiter_delta(ps: &ProcessedSource<'_>, first: Node<'_>, second: Node<'_>) -> i64 {
    if element_same_line(first, second) || delimiter(first, false) != delimiter(second, false) {
        return 0;
    }
    col(ps, first.loc().operator.unwrap().begin_pos)
        - col(ps, second.loc().operator.unwrap().begin_pos)
}

/// `PairNode#delimiter(with_spacing)`.
pub fn delimiter(pair: Node<'_>, with_spacing: bool) -> &'static str {
    match (pair.is_hash_rocket(), with_spacing) {
        (true, true) => " => ",
        (false, true) => ": ",
        (true, false) => "=>",
        (false, false) => ":",
    }
}

/// `PairNode#value_on_new_line?`.
pub fn value_on_new_line(pair: Node<'_>) -> bool {
    pair.key().unwrap().first_line() != pair.pair_value().unwrap().first_line()
}

/// `PairNode#value_omission?`.
pub fn value_omission(pair: Node<'_>) -> bool {
    pair.is(NodeType::Pair) && pair.source().ends_with(':')
}

/// `HashNode#pairs_on_same_line?`.
pub fn pairs_on_same_line(node: Node<'_>) -> bool {
    node.pairs()
        .windows(2)
        .any(|w| element_same_line(w[0], w[1]))
}

/// `HashNode#mixed_delimiters?`.
pub fn mixed_delimiters(node: Node<'_>) -> bool {
    let pairs = node.pairs();
    let mut delimiters = pairs.iter().map(|p| delimiter(*p, false));
    match delimiters.next() {
        Some(first) => delimiters.any(|d| d != first),
        None => false,
    }
}
