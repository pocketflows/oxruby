//! Port of `lib/rubocop/cop/naming/inclusive_language.rb`.

use serde_yaml_ng::Value;

use crate::cop::mixin::{regexp_literal, ruby_pattern};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

/// A flagged term (`@flagged_term_hash` entry).
struct FlaggedTerm {
    regex: onig::Regex,
    suggestions: Option<Value>,
    suggestion_string: String,
}

/// `@flagged_terms_regex`, `@allowed_regex` and `@flagged_term_hash`.
struct Terms {
    flagged_terms_regex: onig::Regex,
    allowed_regex: Option<onig::Regex>,
    flagged_terms: Vec<FlaggedTerm>,
}

struct WordLocation {
    word: String,
}

#[derive(Default)]
pub struct InclusiveLanguage;

impl Cop for InclusiveLanguage {
    fn name(&self) -> &'static str {
        "Naming/InclusiveLanguage"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(terms) = preprocess_flagged_terms(cx) else {
            return;
        };
        if cx.cop_config_bool("CheckFilepaths", false) {
            investigate_filepath(cx, &terms);
        }
        investigate_tokens(cx, &terms);
    }
}

fn ignorecase_regex(source: &str) -> Option<onig::Regex> {
    onig::Regex::with_options(
        &ruby_pattern(source),
        onig::RegexOptions::REGEX_OPTION_IGNORECASE,
        onig::Syntax::ruby(),
    )
    .ok()
}

/// `ensure_regex_string(regex)`: a `Regexp`'s source, or the string.
fn ensure_regex_string(value: &Value) -> Option<String> {
    if let Some((source, _)) = regexp_literal(value) {
        return Some(source.to_owned());
    }
    match value {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn preprocess_flagged_terms(cx: &Investigation<'_, '_>) -> Option<Terms> {
    let mut allowed_strings: Vec<String> = Vec::new();
    let mut flagged_term_strings: Vec<String> = Vec::new();
    let mut flagged_terms = Vec::new();
    let Some(Value::Mapping(terms)) = cx.cop_config_value("FlaggedTerms") else {
        return None;
    };
    for (term, term_definition) in terms {
        let Some(term) = term.as_str() else {
            continue;
        };
        let Value::Mapping(definition) = term_definition else {
            continue;
        };
        // `process_allowed_regex`
        match definition.get("AllowedRegex") {
            None | Some(Value::Null) => {}
            Some(Value::Sequence(items)) => allowed_strings.extend(items.iter().map(allowed_term)),
            Some(other) => allowed_strings.push(allowed_term(other)),
        }
        // `extract_regexp`
        let regex_string = match definition.get("Regex") {
            Some(regex) if !regex.is_null() => ensure_regex_string(regex).unwrap_or_default(),
            _ if definition
                .get("WholeWord")
                .is_some_and(|w| !matches!(w, Value::Null | Value::Bool(false))) =>
            {
                format!(r"(?:\b|(?<=[\W_])){term}(?:\b|(?=[\W_]))")
            }
            _ => term.to_owned(),
        };
        let suggestions = definition.get("Suggestions").cloned();
        flagged_terms.push(FlaggedTerm {
            regex: ignorecase_regex(&regex_string)?,
            suggestion_string: preprocess_suggestions(suggestions.as_ref()),
            suggestions,
        });
        flagged_term_strings.push(regex_string);
    }
    // With no flagged terms an empty regex would match everything, so leave
    // the cop as a no-op.
    if flagged_term_strings.is_empty() {
        return None;
    }
    Some(Terms {
        flagged_terms_regex: ignorecase_regex(&flagged_term_strings.join("|"))?,
        allowed_regex: if allowed_strings.is_empty() {
            None
        } else {
            ignorecase_regex(&allowed_strings.join("|"))
        },
        flagged_terms,
    })
}

/// An `AllowedRegex` entry; blank strings become `nil` (an empty segment).
fn allowed_term(value: &Value) -> String {
    match value {
        Value::String(s) if s.trim().is_empty() => String::new(),
        other => ensure_regex_string(other).unwrap_or_default(),
    }
}

fn investigate_tokens(cx: &mut Investigation<'_, '_>, terms: &Terms) {
    let checks = [
        ("tIDENTIFIER", "CheckIdentifiers"),
        ("tCONSTANT", "CheckConstants"),
        ("tIVAR", "CheckVariables"),
        ("tCVAR", "CheckVariables"),
        ("tGVAR", "CheckVariables"),
        ("tSYMBOL", "CheckSymbols"),
        ("tSTRING", "CheckStrings"),
        ("tSTRING_CONTENT", "CheckStrings"),
        ("tCOMMENT", "CheckComments"),
    ];
    let ps = cx.processed_source;
    let mut offenses: Vec<(Range, String, Option<String>)> = Vec::new();
    for token in ps.tokens() {
        let checked = checks
            .iter()
            .find(|(ty, _)| *ty == token.ty)
            .is_some_and(|(_, key)| cx.cop_config_bool(key, false));
        if !checked {
            continue;
        }
        for word_location in scan_for_words(terms, &token.text) {
            let word = word_location.word;
            let source = ps.slice(token.range);
            let Some(index) = source.find(&word) else {
                continue;
            };
            let start = token.range.begin_pos + index;
            let range = Range::new(start, start + word.len());
            let term = find_flagged_term(terms, &word);
            let message = create_message(term, &word, None);
            let preferred = term.and_then(|t| preferred_sole_term(t.suggestions.as_ref()));
            offenses.push((range, message, preferred));
        }
    }
    for (range, message, preferred) in offenses {
        cx.add_offense_with_correction(range, &message, |corrector| {
            if let Some(preferred) = preferred {
                corrector.replace(range, preferred);
            }
        });
    }
}

fn preferred_sole_term(suggestions: Option<&Value>) -> Option<String> {
    match suggestions? {
        Value::Sequence(items) if items.len() == 1 => preferred_sole_term(items.first()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn investigate_filepath(cx: &mut Investigation<'_, '_>, terms: &Terms) {
    let file_path = cx.processed_source.file_path();
    let word_locations = scan_for_words(terms, &file_path);
    let message = match word_locations.as_slice() {
        [] => return,
        [one] => create_message(
            find_flagged_term(terms, &one.word),
            &one.word,
            Some(" in file path"),
        ),
        many => {
            let words: Vec<&str> = many.iter().map(|w| w.word.as_str()).collect();
            format!(
                "Consider replacing '{}' in file path with other terms.",
                words.join("', '")
            )
        }
    };
    cx.add_global_offense(&message);
}

fn scan_for_words(terms: &Terms, input: &str) -> Vec<WordLocation> {
    let masked_input = mask_input(terms, input);
    terms
        .flagged_terms_regex
        .find_iter(&masked_input)
        .map(|(start, end)| WordLocation {
            word: masked_input[start..end].to_owned(),
        })
        .collect()
}

fn mask_input(terms: &Terms, input: &str) -> String {
    match &terms.allowed_regex {
        None => input.to_owned(),
        Some(allowed) => allowed.replace_all(input, |caps: &onig::Captures<'_>| {
            "*".repeat(caps.at(0).unwrap_or("").chars().count())
        }),
    }
}

fn find_flagged_term<'t>(terms: &'t Terms, word: &str) -> Option<&'t FlaggedTerm> {
    terms
        .flagged_terms
        .iter()
        .find(|term| term.regex.find(word).is_some())
}

fn create_message(term: Option<&FlaggedTerm>, word: &str, in_file_path: Option<&str>) -> String {
    let suggestions = term.map_or("", |t| t.suggestion_string.as_str());
    let suggestions = if suggestions.trim().is_empty() {
        " with another term"
    } else {
        suggestions
    };
    format!(
        "Consider replacing '{word}'{}{suggestions}.",
        in_file_path.unwrap_or("")
    )
}

fn preprocess_suggestions(suggestions: Option<&Value>) -> String {
    let words: Vec<String> = match suggestions {
        None | Some(Value::Null) => return String::new(),
        Some(Value::String(s)) if s.trim().is_empty() => return String::new(),
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Sequence(items)) if items.is_empty() => return String::new(),
        Some(Value::Sequence(items)) => items
            .iter()
            .map(|i| i.as_str().unwrap_or("").to_owned())
            .collect(),
        Some(_) => return String::new(),
    };
    let mut quoted: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
    let suggestion_str = match quoted.len() {
        1 => quoted.remove(0),
        2 => quoted.join(" or "),
        _ => {
            let last = quoted.pop().unwrap();
            quoted.push(format!("or {last}"));
            quoted.join(", ")
        }
    };
    format!(" with {suggestion_str}")
}
