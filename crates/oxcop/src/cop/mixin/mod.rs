//! Ports of `lib/rubocop/cop/mixin/*.rb` and `lib/rubocop/cop/util.rb`:
//! helpers shared by cops. Mixin methods that need the cop's state take the
//! `Investigation` (`cx`) as their first argument.

pub mod allowed_methods;
pub mod allowed_pattern;
pub mod comments_help;
pub mod configurable_enforced_style;
pub mod configurable_naming;
pub mod frozen_string_literal;
pub mod heredoc;
pub mod ignored_node;
pub mod integer_node;
pub mod negative_conditional;
pub mod parentheses;
pub mod range_help;
pub mod safe_assignment;
pub mod uncommunicative_name;
pub mod util;
pub mod visibility_help;

/// A lazily compiled Ruby (Onigmo-syntax) regexp, like a Ruby regexp
/// literal: `regex!(r"\A\s*#")`.
#[macro_export]
macro_rules! regex {
    ($re:expr) => {{
        static RE: std::sync::LazyLock<onig::Regex> = std::sync::LazyLock::new(|| {
            onig::Regex::with_options(
                &$crate::cop::mixin::ruby_pattern($re),
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
        &ruby_pattern(pattern),
        onig::RegexOptions::REGEX_OPTION_NONE,
        onig::Syntax::ruby(),
    )
    .ok()
}

/// Rewrites a Ruby regexp source so that Oniguruma matches it like Ruby's
/// Onigmo: `\w`, `\d` and `\s` (and their negations) are ASCII-only in
/// Ruby, while POSIX brackets (`[[:word:]]`, `[[:upper:]]`) and `\b` stay
/// Unicode-aware. Oniguruma's `*_IS_ASCII` options would also make the POSIX
/// brackets ASCII, so the shorthands are spelled out as (nested) classes.
pub fn ruby_pattern(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            out.push(c);
            break;
        };
        match next {
            'w' => out.push_str("[a-zA-Z0-9_]"),
            'W' => out.push_str("[^a-zA-Z0-9_]"),
            'd' => out.push_str("[0-9]"),
            'D' => out.push_str("[^0-9]"),
            's' => out.push_str("[ \\t\\n\\v\\f\\r]"),
            'S' => out.push_str("[^ \\t\\n\\v\\f\\r]"),
            other => {
                out.push(c);
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn ruby_character_classes() {
        assert!(crate::regex!(r"\w").find("é").is_none());
        assert!(crate::regex!(r"\d").find("٣").is_none());
        assert!(crate::regex!(r"\s").find("\u{a0}").is_none());
        assert!(crate::regex!(r"[\w-]+").find("é").is_none());
        assert!(crate::regex!(r"[^\W]").find("é").is_none());
        assert!(crate::regex!(r"\A[\w\s]+\z").find("a b").is_some());
        assert!(crate::regex!(r"\\w").find("\\w").is_some());
        assert!(crate::regex!(r"[[:word:]]").find("é").is_some());
        assert!(crate::regex!(r"[[:upper:]]").find("É").is_some());
        assert!(crate::regex!(r"\bfoo\b").find("éfoo").is_none());
    }
}
