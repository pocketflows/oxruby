//! Port of `lib/rubocop/cop/mixin/configurable_numbering.rb`.

use crate::regex;

/// `FORMATS` of `ConfigurableNumbering`.
pub fn numbering_format(style: &str) -> Option<&'static onig::Regex> {
    match style {
        "snake_case" => Some(regex!(r"(?:\D|_\d+|\A\d+)\z")),
        "normalcase" => Some(regex!(r"(?:\D|[^_\d]\d+|\A\d+)\z|\A_\d+\z")),
        "non_integer" => Some(regex!(r"(\D|\A\d+)\z|\A_\d+\z")),
        _ => None,
    }
}
