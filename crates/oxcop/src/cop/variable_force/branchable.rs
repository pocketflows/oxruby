//! Port of `lib/rubocop/cop/variable_force/branchable.rb`.

use crate::cop::variable_force::branch::Branch;

/// `run_exclusively_with?(other)` for two branchable objects (assignments
/// and references), given their branches.
pub fn run_exclusively_with<'a>(branch: Option<&Branch<'a>>, other: Option<&Branch<'a>>) -> bool {
    match (branch, other) {
        (Some(branch), Some(other)) => branch.exclusive_with(Some(other)),
        _ => false,
    }
}
