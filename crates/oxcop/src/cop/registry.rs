//! Port of `lib/rubocop/cop/registry.rb`. Every ported cop is listed here.

use std::sync::LazyLock;

use crate::cop::{CONSTRUCTORS, Cop};

pub type CopConstructor = fn() -> Box<dyn Cop>;

/// Every ported cop, sorted by name like RuboCop's registry (departments in
/// alphabetical order, cops alphabetically within them).
pub fn all() -> &'static [CopConstructor] {
    static SORTED: LazyLock<Vec<CopConstructor>> = LazyLock::new(|| {
        let mut cops: Vec<(String, CopConstructor)> = CONSTRUCTORS
            .iter()
            .map(|new| (new().name().to_owned(), *new))
            .collect();
        cops.sort_by(|a, b| a.0.cmp(&b.0));
        cops.into_iter().map(|(_, new)| new).collect()
    });
    &SORTED
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
