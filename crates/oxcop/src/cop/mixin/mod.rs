//! Ports of `lib/rubocop/cop/mixin/*.rb` and `lib/rubocop/cop/util.rb`:
//! helpers shared by cops. Mixin methods that need the cop's state take the
//! `Investigation` (`cx`) as their first argument.

pub mod allowed_methods;
pub mod allowed_pattern;
pub mod comments_help;
pub mod configurable_enforced_style;
pub mod configurable_naming;
pub mod frozen_string_literal;
pub mod ignored_node;
pub mod negative_conditional;
pub mod parentheses;
pub mod range_help;
pub mod safe_assignment;
pub mod util;
pub mod visibility_help;

/// A lazily compiled Ruby (Onigmo-syntax) regexp, like a Ruby regexp
/// literal: `regex!(r"\A\s*#")`.
#[macro_export]
macro_rules! regex {
    ($re:expr) => {{
        static RE: std::sync::LazyLock<onig::Regex> = std::sync::LazyLock::new(|| {
            onig::Regex::with_options(
                $re,
                onig::RegexOptions::REGEX_OPTION_NONE,
                onig::Syntax::ruby(),
            )
            .unwrap()
        });
        &*RE
    }};
}

/// Compiles a regexp given as a string at run time (`Regexp.new(string)`).
pub fn ruby_regex(pattern: &str) -> Option<onig::Regex> {
    onig::Regex::with_options(
        pattern,
        onig::RegexOptions::REGEX_OPTION_NONE,
        onig::Syntax::ruby(),
    )
    .ok()
}
