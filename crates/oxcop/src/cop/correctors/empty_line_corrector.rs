//! Port of `lib/rubocop/cop/correctors/empty_line_corrector.rb`.

use crate::cop::Corrector;
use crate::processed_source::Range;

/// The `offense_style` of `EmptyLineCorrector.correct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffenseStyle {
    NoEmptyLines,
    EmptyLines,
}

/// `EmptyLineCorrector.correct(corrector, [offense_style, range])`.
pub fn correct(corrector: &mut Corrector, offense_style: OffenseStyle, range: Range) {
    match offense_style {
        OffenseStyle::NoEmptyLines => corrector.remove(range),
        OffenseStyle::EmptyLines => corrector.insert_before(range, "\n"),
    }
}

/// `EmptyLineCorrector.insert_before(corrector, node)`.
pub fn insert_before(corrector: &mut Corrector, range: Range) {
    corrector.insert_before(range, "\n");
}
