//! Port of `lib/rubocop/version.rb`.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The RuboCop release whose behaviour (and `config/default.yml`) we track.
pub const RUBOCOP_VERSION: &str = "1.91.0";
