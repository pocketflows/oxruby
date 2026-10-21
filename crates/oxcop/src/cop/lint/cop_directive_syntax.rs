//! Port of `lib/rubocop/cop/lint/cop_directive_syntax.rb`.

use std::sync::LazyLock;

use onig::Regex;

use crate::comment_config::DirectiveScopes;
use crate::cop::base::raise;
use crate::cop::{Cop, Investigation};
use crate::directive_comment::{
    modes_pattern, CopRegistry, DirectiveComment, AVAILABLE_MODES, DIRECTIVE_COMMENT_REGEXP,
    DIRECTIVE_MARKER_REGEXP,
};
use crate::name_similarity::{find_similar_name, levenshtein_distance};
use crate::processed_source::Comment;

const COMMON_MSG: &str = "Malformed directive comment detected.";

const MISSING_MODE_NAME_MSG: &str = "The mode name is missing.";
const INVALID_MODE_NAME_MSG: &str = "The mode name must be one of `enable`, `disable`, `disable-next`, `enable-next`, `todo`, `todo-next`, `next`, `push`, or `pop`.";
const MISSING_COP_NAME_MSG: &str = "The cop name is missing.";
const MULTIPLE_DIRECTIVES_MSG: &str = "Only the first directive on a line takes effect. \
                                       List the cop names in a single directive instead.";
const MALFORMED_COP_NAMES_MSG: &str = "Cop names must be separated by commas. \
                                       Comment in the directive must start with `--`.";
const INVALID_SIGNED_ARGS_MSG: &str = "`push` and `next` arguments must be `+`- or `-`-prefixed \
                                       cop names, and `pop` takes no arguments.";
const NEXT_DIRECTIVE_AT_EOL_MSG: &str = "A `-next` directive must be on its own line, above the \
                                         statement it applies to.";
const INVALID_KEYWORD_MSG: &str = "The directive keyword must be `rubocop`, not `%<keyword>s`.";
const UNKNOWN_COP_MSG: &str = "Unknown cop name `%<name>s`%<suggestion>s.";

/// A comment that looks like an attempted directive: any keyword
/// followed by a colon and a valid mode name.
static NEAR_MISS_KEYWORD_REGEXP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\A#+\s*(?<keyword>[A-Za-z][A-Za-z0-9_-]*)\s*:\s*(?:{})\b",
        modes_pattern()
    ))
    .unwrap()
});

#[derive(Default)]
pub struct CopDirectiveSyntax;

impl Cop for CopDirectiveSyntax {
    fn name(&self) -> &'static str {
        "Lint/CopDirectiveSyntax"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let mut scopes = DirectiveScopes::new(ps, cx.config);
        for comment in ps.comments() {
            let line = ps.line_of(comment.range.begin_pos) as i64;
            let directive_comment = DirectiveComment::new(comment.text, comment.range, line);
            if directive_comment.start_with_marker() {
                check_directive(cx, &mut scopes, comment, &directive_comment);
            } else if let Some(keyword) = near_miss_keyword(comment) {
                let message = format!("{COMMON_MSG} {INVALID_KEYWORD_MSG}")
                    .replace("%<keyword>s", keyword);
                cx.add_offense(comment.range, &message);
            }
        }
    }
}

/// A `DIRECTIVE_COMMENT_REGEXP` match: its bounds (bytes in the comment
/// text), the mode (`match[1]`) and the cop names (`match[2]`).
struct Match<'t> {
    begin: usize,
    end: usize,
    mode: &'t str,
    cops: Option<&'t str>,
}

fn check_directive(
    cx: &mut Investigation<'_, '_>,
    scopes: &mut DirectiveScopes<'_, '_>,
    comment: &Comment<'_>,
    directive_comment: &DirectiveComment<'_>,
) {
    if let Some(directives) = multiple_directives(comment, directive_comment) {
        let message = format!("{COMMON_MSG} {MULTIPLE_DIRECTIVES_MSG}");
        let replacement = merge_directives(comment, &directives);
        cx.add_offense_with_correction(comment.range, &message, |corrector| {
            if let Some(replacement) = replacement {
                corrector.replace(comment.range, replacement);
            }
        });
    } else if directive_comment.malformed() {
        cx.add_offense(comment.range, &offense_message(directive_comment));
    } else if misplaced_next_directive(scopes, directive_comment) {
        cx.add_offense(comment.range, NEXT_DIRECTIVE_AT_EOL_MSG);
    } else {
        let registry = scopes.registry();
        if let Some(name) = unknown_cop_name(registry, directive_comment) {
            cx.add_offense(comment.range, &unknown_cop_message(registry, &name));
        }
    }
}

/// Everything after the first directive in a comment is trailing text as
/// far as the parser is concerned, so a second directive written on the
/// same line disables nothing at all. A valid `--` reason may mention a
/// directive as plain text, which should not be treated as another one.
fn multiple_directives<'t>(
    comment: &Comment<'t>,
    directive_comment: &DirectiveComment<'_>,
) -> Option<Vec<Match<'t>>> {
    let text = comment.text;
    let matches: Vec<Match<'t>> = DIRECTIVE_COMMENT_REGEXP
        .captures_iter(text)
        .filter_map(|captures| {
            let (begin, end) = captures.pos(0)?;
            Some(Match {
                begin,
                end,
                mode: captures.at(1).unwrap_or(""),
                cops: captures.at(2),
            })
        })
        .collect();
    let matches = filter_reason_matches(text, matches, directive_comment);
    (matches.len() > 1).then_some(matches)
}

fn filter_reason_matches<'t>(
    text: &str,
    matches: Vec<Match<'t>>,
    directive_comment: &DirectiveComment<'_>,
) -> Vec<Match<'t>> {
    if directive_comment.reason().is_none() {
        return matches;
    }
    matches
        .into_iter()
        .enumerate()
        .filter(|(index, m)| *index == 0 || !text[m.end..].chars().any(|c| !is_space(c)))
        .map(|(_, m)| m)
        .collect()
}

/// The comment's replacement, `None` when the directives can't be merged.
fn merge_directives(comment: &Comment<'_>, directives: &[Match<'_>]) -> Option<String> {
    if !directives_adjacent(comment, directives) {
        return None;
    }
    let mode = merged_mode(directives)?;
    let cops = merged_cops(directives)?;
    Some(format!("# rubocop:{mode} {}", cops.join(", ")))
}

/// `all` covers every cop already, so folding it into a list of names
/// would be a rewrite rather than a merge.
fn merged_cops(directives: &[Match<'_>]) -> Option<Vec<String>> {
    let mut cops: Vec<String> = Vec::new();
    for directive in directives {
        for cop in directive.cops.unwrap_or("").split(',') {
            let cop = ruby_strip(cop);
            if !cop.is_empty() && !cops.iter().any(|c| c == cop) {
                cops.push(cop.to_owned());
            }
        }
    }
    (!(cops.is_empty() || cops.iter().any(|c| c == "all"))).then_some(cops)
}

/// Anything written between the directives is a `--` reason or prose,
/// and merging around it would either move it or drop it.
fn directives_adjacent(comment: &Comment<'_>, directives: &[Match<'_>]) -> bool {
    let text = comment.text;
    let froms = directives.iter().map(|m| m.end);
    let tos = directives
        .iter()
        .skip(1)
        .map(|m| m.begin)
        .chain(std::iter::once(text.len()));
    froms
        .zip(tos)
        .all(|(from, to)| text.get(from..to).is_none_or(|s| ruby_strip(s).is_empty()))
}

/// `todo` is an alias of `disable`, so those two combine. Anything else
/// (an `enable` beside a `disable`, or directives of different scopes)
/// means different things per directive and can't be folded into one.
fn merged_mode<'t>(directives: &[Match<'t>]) -> Option<&'t str> {
    let mut modes: Vec<&str> = Vec::new();
    for directive in directives {
        if !modes.contains(&directive.mode) {
            modes.push(directive.mode);
        }
    }
    if modes.len() == 1 {
        return modes.first().copied();
    }
    if modes.iter().all(|m| matches!(*m, "disable" | "todo")) {
        return modes.first().copied();
    }
    None
}

/// An EOL next-statement directive is not honored - it must fail
/// loudly instead of silently doing nothing.
fn misplaced_next_directive(
    scopes: &mut DirectiveScopes<'_, '_>,
    directive_comment: &DirectiveComment<'_>,
) -> bool {
    next_statement_directive(directive_comment)
        && !scopes.comment_only_line(directive_comment.line_number)
}

fn next_statement_directive(directive_comment: &DirectiveComment<'_>) -> bool {
    directive_comment.disable_next() || directive_comment.enable_next() || directive_comment.next()
}

fn offense_message(directive_comment: &DirectiveComment<'_>) -> String {
    let text = directive_comment.comment_text;
    let after_marker = match DIRECTIVE_MARKER_REGEXP.find(text) {
        Some((begin, end)) => format!("{}{}", &text[..begin], &text[end..]),
        None => text.to_owned(),
    };
    // `split(' ', 2).first`
    let mode = after_marker.split(is_space).find(|s| !s.is_empty());
    let additional_msg = match mode {
        None => MISSING_MODE_NAME_MSG,
        Some(mode) if !AVAILABLE_MODES.contains(&mode) => INVALID_MODE_NAME_MSG,
        _ if directive_comment.missing_cop_name() => MISSING_COP_NAME_MSG,
        _ if directive_comment.invalid_signed_args() => INVALID_SIGNED_ARGS_MSG,
        _ => MALFORMED_COP_NAMES_MSG,
    };
    format!("{COMMON_MSG} {additional_msg}")
}

fn near_miss_keyword<'t>(comment: &Comment<'t>) -> Option<&'t str> {
    let captures = NEAR_MISS_KEYWORD_REGEXP.captures(comment.text)?;
    let keyword = captures.at(1)?;
    if keyword == "rubocop" {
        return None;
    }
    similar_to_rubocop(keyword).then_some(keyword)
}

fn similar_to_rubocop(keyword: &str) -> bool {
    if keyword.eq_ignore_ascii_case("rubocop") {
        return true;
    }
    levenshtein_distance(&keyword.to_lowercase(), "rubocop") <= 2
}

fn unknown_cop_name(
    registry: &dyn CopRegistry,
    directive_comment: &DirectiveComment<'_>,
) -> Option<String> {
    if directive_comment.pop() || directive_comment.all_cops() {
        return None;
    }
    directive_names(directive_comment).into_iter().find(|name| {
        if registry.is_department(name) {
            return false;
        }
        let qualified = qualified_cop_name(registry, name);
        !contains_cop_matching(registry, &qualified)
    })
}

fn directive_names(directive_comment: &DirectiveComment<'_>) -> Vec<String> {
    if directive_comment.push() || directive_comment.next() {
        directive_comment
            .signed_args()
            .into_iter()
            .flat_map(|(_, names)| names)
            .collect()
    } else {
        directive_comment.raw_cop_names()
    }
}

fn unknown_cop_message(registry: &dyn CopRegistry, name: &str) -> String {
    let names: Vec<&str> = registry.names().iter().map(String::as_str).collect();
    let suggestion = match find_similar_name(name, &names) {
        Some(similar) => format!(" (did you mean `{similar}`?)"),
        None => String::new(),
    };
    UNKNOWN_COP_MSG
        .replace("%<name>s", name)
        .replace("%<suggestion>s", &suggestion)
}

/// Ruby's `\s` (and `split(' ')`'s separators).
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// `String#strip`, which also removes NUL.
fn ruby_strip(s: &str) -> &str {
    s.trim_matches(|c| is_space(c) || c == '\0')
}

/// `Badge.camel_case(name_part)`.
fn camel_case(name_part: &str) -> String {
    if name_part == "rspec" {
        return "RSpec".to_owned();
    }
    let bytes = name_part.as_bytes();
    let mut out = String::with_capacity(name_part.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if i == 0 && c.is_ascii_lowercase() {
            out.push(c.to_ascii_uppercase() as char);
            i += 1;
        } else if c == b'_' && bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase) {
            out.push(bytes[i + 1].to_ascii_uppercase() as char);
            i += 2;
        } else {
            let ch = name_part[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// `Badge.parse(name)`: its department (if any) and cop name.
fn parse_badge(name: &str) -> (Option<String>, String) {
    let mut parts: Vec<String> = name.split('/').map(camel_case).collect();
    let cop_name = parts.pop().unwrap_or_default();
    let department = (!parts.is_empty()).then(|| parts.join("/"));
    (department, cop_name)
}

/// `Registry#registered?(badge)`.
fn registered(registry: &dyn CopRegistry, department: Option<&str>, cop_name: &str) -> bool {
    department.is_some_and(|department| {
        let name = format!("{department}/{cop_name}");
        registry.names().iter().any(|n| *n == name)
    })
}

/// `registry.qualified_cop_name(name, nil, warn: false)`.
fn qualified_cop_name(registry: &dyn CopRegistry, name: &str) -> String {
    let (department, cop_name) = parse_badge(name);
    if registered(registry, department.as_deref(), &cop_name) {
        return name.to_owned();
    }
    // `qualify_badge(badge)`: the registry's departments, in order.
    let mut departments: Vec<&str> = Vec::new();
    for registered_name in registry.names() {
        if let Some((department, _)) = registered_name.rsplit_once('/')
            && !departments.contains(&department)
        {
            departments.push(department);
        }
    }
    let potential_badges: Vec<String> = departments
        .into_iter()
        .filter(|department| registered(registry, Some(department), &cop_name))
        .map(|department| format!("{department}/{cop_name}"))
        .collect();
    match potential_badges.as_slice() {
        [] => name.to_owned(),
        [badge] => badge.clone(),
        _ => {
            let badge = match &department {
                Some(department) => format!("{department}/{cop_name}"),
                None => cop_name,
            };
            raise(
                "RuboCop::Cop::AmbiguousCopName",
                format!(
                    "Ambiguous cop name `{badge}` used in  needs department qualifier. \
                     Did you mean {}?",
                    potential_badges.join(" or ")
                ),
            )
        }
    }
}

/// `registry.contains_cop_matching?([name])`.
fn contains_cop_matching(registry: &dyn CopRegistry, name: &str) -> bool {
    registry.names().iter().any(|registered_name| {
        registered_name == name
            || registered_name
                .rsplit_once('/')
                .is_some_and(|(department, _)| department == name)
    })
}
