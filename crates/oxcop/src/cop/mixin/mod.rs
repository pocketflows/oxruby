//! Ports of `lib/rubocop/cop/mixin/*.rb` and `lib/rubocop/cop/util.rb`:
//! helpers shared by cops. Mixin methods that need the cop's state take the
//! `Investigation` (`cx`) as their first argument.

pub mod alignment;
pub mod allowed_methods;
pub mod allowed_pattern;
pub mod allowed_receivers;
pub mod annotation_comment;
pub mod array_min_size;
pub mod array_syntax;
pub mod check_assignment;
pub mod check_line_breakable;
pub mod check_single_line_suitability;
pub mod code_length;
pub mod comments_help;
pub mod configurable_enforced_style;
pub mod configurable_naming;
pub mod configurable_numbering;
pub mod def_node;
pub mod dig_help;
pub mod documentation_comment;
pub mod duplication;
pub mod empty_lines_around_body;
pub mod empty_parameter;
pub mod end_keyword_alignment;
pub mod endless_method_rewriter;
pub mod first_element_line_break;
pub mod frozen_string_literal;
pub mod gem_declaration;
pub mod gemspec_help;
pub mod hash_alignment_styles;
pub mod hash_shorthand_syntax;
pub mod hash_subset;
pub mod hash_transform_method;
pub mod heredoc;
pub mod ignored_node;
pub mod integer_node;
pub mod interpolation;
pub mod line_length_help;
pub mod match_range;
pub mod method_complexity;
pub mod method_preference;
pub mod min_body_length;
pub mod min_branches_count;
pub mod multiline_element_indentation;
pub mod multiline_element_line_breaks;
pub mod multiline_expression_indentation;
pub mod multiline_literal_brace_layout;
pub mod negative_conditional;
pub mod nil_methods;
pub mod on_normal_if_unless;
pub mod ordered_gem_node;
pub mod parentheses;
pub mod percent_array;
pub mod percent_literal;
pub mod preceding_following_alignment;
pub mod preferred_delimiters;
pub mod range_help;
pub mod rational_literal;
pub mod reparsed_equivalence;
pub mod require_library;
pub mod rescue_node;
pub mod safe_assignment;
pub mod space_after_punctuation;
pub mod space_before_punctuation;
pub mod statement_modifier;
pub mod string_help;
pub mod string_literals_help;
pub mod surrounding_space;
pub mod symbol_help;
pub mod trailing_body;
pub mod trailing_comma;
pub mod uncommunicative_name;
pub mod unused_argument;
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

/// A `Regexp` in a cop's configuration. Specs pass Ruby `Regexp` objects
/// (`'AllowedPatterns' => [/foo/i]`), which reach the binary as YAML
/// `!ruby/regexp /foo/i` values; configuration files use strings, which
/// `Regexp.new` compiles.
pub fn config_regexp(value: &serde_yaml_ng::Value) -> Option<onig::Regex> {
    match value {
        serde_yaml_ng::Value::String(source) => ruby_regex(source),
        serde_yaml_ng::Value::Tagged(_) => {
            let (source, flags) = regexp_literal(value)?;
            let mut options = onig::RegexOptions::REGEX_OPTION_NONE;
            if flags.contains('i') {
                options |= onig::RegexOptions::REGEX_OPTION_IGNORECASE;
            }
            if flags.contains('x') {
                options |= onig::RegexOptions::REGEX_OPTION_EXTEND;
            }
            if flags.contains('m') {
                options |= onig::RegexOptions::REGEX_OPTION_MULTILINE;
            }
            onig::Regex::with_options(&ruby_pattern(source), options, onig::Syntax::ruby()).ok()
        }
        _ => None,
    }
}

/// The source and flags of a `!ruby/regexp /source/flags` value.
pub fn regexp_literal(value: &serde_yaml_ng::Value) -> Option<(&str, &str)> {
    let serde_yaml_ng::Value::Tagged(tagged) = value else {
        return None;
    };
    if tagged.tag != "ruby/regexp" {
        return None;
    }
    tagged.value.as_str()?.strip_prefix('/')?.rsplit_once('/')
}

/// `Regexp#to_s` of a `!ruby/regexp` value: `(?i-mx:source)`.
pub fn regexp_to_s(value: &serde_yaml_ng::Value) -> Option<String> {
    let (source, flags) = regexp_literal(value)?;
    let on: String = "mix".chars().filter(|c| flags.contains(*c)).collect();
    let off: String = "mix".chars().filter(|c| !flags.contains(*c)).collect();
    Some(if off.is_empty() {
        format!("(?{on}:{source})")
    } else {
        format!("(?{on}-{off}:{source})")
    })
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
