//! Port of `lib/rubocop/cop/correctors/percent_literal_corrector.rb`: turns
//! a bracketed array of words or symbols into a `%w`/`%i` literal.

use crate::ast::{Child, Node};
use crate::config::Config;
use crate::cop::Corrector;
use crate::cop::mixin::preferred_delimiters::delimiters;
use crate::cop::mixin::util::{
    double_quotes_required, escape_string, needs_escaping, symbol_inspect,
};
use crate::regex;

/// `PercentLiteralCorrector.new(config, nil).correct(corrector, node, char)`.
pub fn correct(config: &Config, corrector: &mut Corrector, node: Node<'_>, char: &str) {
    let escape = escape_words(node);
    let char = if escape {
        char.to_uppercase()
    } else {
        char.to_owned()
    };
    let Some(delimiters) = delimiters(config, &format!("%{char}")) else {
        return;
    };
    let contents = new_contents(node, escape, &delimiters);
    corrector.replace(
        node.source_range(),
        format!("%{char}{}{contents}{}", delimiters.0, delimiters.1),
    );
}

/// `needs_escaping?(w.children[0])` for a string or symbol value.
fn value_needs_escaping(value: Option<&Child>) -> bool {
    match value {
        Some(Child::Str(s)) => needs_escaping(s),
        Some(Child::Sym(s)) => {
            // `escape_string` calls `inspect` on the Symbol.
            let inspected = symbol_inspect(s);
            let inner = inspected
                .get(1..inspected.len().saturating_sub(1))
                .unwrap_or_default()
                .replace("\\\"", "\"");
            double_quotes_required(&inner)
        }
        _ => false,
    }
}

fn escape_words(node: Node<'_>) -> bool {
    node.child_nodes()
        .any(|w| value_needs_escaping(w.children().first()))
}

fn new_contents(node: Node<'_>, escape: bool, delimiters: &(String, String)) -> String {
    if node.is_multiline() {
        autocorrect_multiline_words(node, escape, delimiters)
    } else {
        node.child_nodes()
            .map(|word| fix_escaped_content(word, escape, delimiters))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn autocorrect_multiline_words(
    node: Node<'_>,
    escape: bool,
    delimiters: &(String, String),
) -> String {
    let mut contents = process_multiline_words(node, escape, delimiters);
    if let Some(end) = end_content(node.source()) {
        contents.push(end);
    }
    contents.concat()
}

fn process_multiline_words(
    node: Node<'_>,
    escape: bool,
    delimiters: &(String, String),
) -> Vec<String> {
    let base_line_num = node.first_line();
    let mut prev_line_num = base_line_num;
    let source_in_lines: Vec<&str> = ruby_split_lines(node.source());
    node.child_nodes()
        .enumerate()
        .map(|(index, word)| {
            let line_breaks = if word.first_line() == prev_line_num {
                if index == 0 && word.first_line() == base_line_num {
                    String::new()
                } else {
                    " ".to_owned()
                }
            } else {
                process_lines(word, prev_line_num, base_line_num, &source_in_lines)
            };
            prev_line_num = word.last_line();
            line_breaks + &fix_escaped_content(word, escape, delimiters)
        })
        .collect()
}

/// `source.split("\n")`: trailing empty strings are dropped.
fn ruby_split_lines(source: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = source.split('\n').collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn process_lines(
    node: Node<'_>,
    previous_line_num: usize,
    base_line_num: usize,
    lines: &[&str],
) -> String {
    let begin = (previous_line_num - base_line_num + 1).min(lines.len());
    let end = (node.first_line() - base_line_num + 1)
        .min(lines.len())
        .max(begin);
    let joined = lines[begin..end].join("\n");
    // `joined.split(node.source).first || ''`.
    let before = match joined.find(node.source()) {
        Some(i) => &joined[..i],
        None => joined.as_str(),
    };
    format!("\n{before}")
}

fn fix_escaped_content(word: Node<'_>, escape: bool, delimiters: &(String, String)) -> String {
    let mut content = match word.children().first() {
        Some(Child::Str(s) | Child::Sym(s)) => s.clone(),
        _ => String::new(),
    };
    if escape {
        content = escape_string(&content);
    }
    substitute_escaped_delimiters(content, delimiters)
}

fn substitute_escaped_delimiters(mut content: String, delimiters: &(String, String)) -> String {
    let (open, close) = delimiters;
    if open != close
        && content.matches(open.as_str()).count() == content.matches(close.as_str()).count()
    {
        return content;
    }
    for delim in [open, close] {
        if delim.is_empty() {
            continue;
        }
        content = content.replace(delim.as_str(), &format!("\\{delim}"));
    }
    content
}

fn end_content(source: &str) -> Option<String> {
    let last = ruby_split_lines(source).last().copied().unwrap_or_default();
    let captures = regex!(r"\A(\s*)\]").captures(last)?;
    Some(format!("\n{}", captures.at(1).unwrap_or_default()))
}
