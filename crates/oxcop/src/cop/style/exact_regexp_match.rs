//! Port of `lib/rubocop/cop/style/exact_regexp_match.rb`.
//!
//! RuboCop parses the regexp with the `regexp_parser` gem and checks for
//! the tokens `bos`, `literal`, `eos` with no quantifier. Without a port of
//! that gem, [`exact_match_literal`] recognizes the same shape directly:
//! `\A`, a run of characters `regexp_parser` lexes as one literal, `\z`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static EXACT_REGEXP_MATCH: NodePattern = NodePattern::new(
    "(call
       _ {:=~ :=== :!~ :match :match?}
       (regexp
         (str $_)
         (regopt)))",
);

#[derive(Default)]
pub struct ExactRegexpMatch;

impl Cop for ExactRegexpMatch {
    fn name(&self) -> &'static str {
        "Style/ExactRegexpMatch"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["=~", "===", "!~", "match", "match?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(captures) = EXACT_REGEXP_MATCH.match_node(node) else {
            return;
        };
        let Some(regexp) = captures[0].str() else {
            return;
        };
        let Some(literal) = exact_match_literal(regexp) else {
            return;
        };
        let string = escape_single_quotes(literal);
        let new_method = if node.is_method("!~") { "!=" } else { "==" };
        let prefer = format!("{} {new_method} '{string}'", receiver.source());
        let message = format!("Use `{prefer}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), prefer);
        });
    }
}

/// The literal of a regexp that is exactly `\A<literal>\z`.
fn exact_match_literal(regexp: &str) -> Option<&str> {
    let literal = regexp.strip_prefix(r"\A")?.strip_suffix(r"\z")?;
    if literal.is_empty() {
        return None;
    }
    // Any metacharacter, escape, group, class or quantifier makes the body
    // something other than a single unquantified literal.
    let special = |c: char| {
        matches!(
            c,
            '\\' | '.' | '^' | '$' | '|' | '(' | ')' | '[' | ']' | '{' | '}' | '*' | '+' | '?'
        ) || c.is_whitespace() && c != ' '
    };
    if literal.chars().any(special) {
        return None;
    }
    Some(literal)
}

fn escape_single_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\'' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
