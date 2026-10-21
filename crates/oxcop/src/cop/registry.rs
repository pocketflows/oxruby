//! Port of `lib/rubocop/cop/registry.rb`. Every ported cop is listed here.

use crate::cop::{Cop, layout, lint};

pub type CopConstructor = fn() -> Box<dyn Cop>;

pub fn all() -> &'static [CopConstructor] {
    &[
        || Box::new(layout::TrailingWhitespace),
        || Box::new(lint::Syntax),
    ]
}

pub fn names() -> Vec<&'static str> {
    all().iter().map(|new| new().name()).collect()
}

/// Port of `Cop::Base.match?`: matches a cop name or a department name.
pub fn matches(cop_name: &str, given_names: &[String]) -> bool {
    let department = crate::cop::base::department(cop_name);
    given_names
        .iter()
        .any(|name| name == cop_name || name == department)
}
