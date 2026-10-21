//! Port of `lib/rubocop/cop/layout/space_inside_reference_brackets.rb`.

use crate::ast::token::Token;
use crate::ast::{Node, NodeType};
use crate::cop::correctors::space_corrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::surrounding_space::{
    SurroundingSpace, empty_brackets, empty_offenses, no_space_offenses, space_offenses,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "{} space inside reference brackets.";
const EMPTY_MSG: &str = "{} space inside empty reference brackets.";

#[derive(Default)]
pub struct SpaceInsideReferenceBrackets {
    ignored_nodes: IgnoredNodes,
}

impl Cop for SpaceInsideReferenceBrackets {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideReferenceBrackets"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]", "[]="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ps = cx.processed_source;
        let tokens = ps.tokens_within(node.source_range());
        let Some(left_token) = left_ref_bracket(ps, node, tokens) else {
            return;
        };
        let right_token = closing_bracket(tokens, left_token);
        if empty_brackets(Some(left_token), right_token, tokens) {
            let empty_config = empty_config(cx);
            empty_offenses(
                self,
                cx,
                node,
                left_token,
                right_token.unwrap(),
                EMPTY_MSG,
                empty_config,
            );
            return;
        }
        if node.is_multiline() {
            return;
        }
        if style(cx) == "no_space" {
            no_space_offenses(self, cx, node, Some(left_token), right_token, MSG, false, false);
        } else {
            space_offenses(self, cx, node, Some(left_token), right_token, MSG, false, false);
        }
    }
}

impl SurroundingSpace for SpaceInsideReferenceBrackets {
    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>) {
        let ps = cx.processed_source;
        let tokens = ps.tokens_within(node.source_range());
        let Some(left) = left_ref_bracket(ps, node, tokens) else {
            return;
        };
        let Some(right) = closing_bracket(tokens, left) else {
            return;
        };
        if empty_brackets(Some(left), Some(right), tokens) {
            space_corrector::empty_corrections(ps, corrector, empty_config(cx), left, right);
        } else if style(cx) == "no_space" {
            space_corrector::remove_space(ps, corrector, left, right);
        } else {
            space_corrector::add_space(ps, corrector, left, right);
        }
    }

    fn ignored_nodes(&mut self) -> &mut IgnoredNodes {
        &mut self.ignored_nodes
    }
}

fn left_ref_bracket<'t>(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    tokens: &'t [Token],
) -> Option<&'t Token> {
    let current_token = tokens.iter().rev().find(|t| t.is_left_ref_bracket());
    let previous_token = current_token.and_then(|t| previous_token(ps, t));
    if node.is_method("[]=") || previous_token.is_some_and(|t| !t.is_right_bracket()) {
        tokens.iter().find(|t| t.is_left_ref_bracket())
    } else {
        current_token
    }
}

fn closing_bracket<'t>(tokens: &'t [Token], opening_bracket: &Token) -> Option<&'t Token> {
    let i = tokens.iter().position(|t| t == opening_bracket)?;
    let mut inner_left_brackets_needing_closure = 0isize;
    for token in &tokens[i..] {
        if token.is_left_bracket() {
            inner_left_brackets_needing_closure += 1;
        }
        if token.is_right_bracket() {
            inner_left_brackets_needing_closure -= 1;
        }
        if inner_left_brackets_needing_closure == 0 && token.is_right_bracket() {
            return Some(token);
        }
    }
    None
}

fn previous_token<'p>(ps: &'p ProcessedSource<'_>, current_token: &Token) -> Option<&'p Token> {
    let tokens = ps.tokens();
    let index = tokens.iter().position(|t| t == current_token)?;
    index.checked_sub(1).map(|i| &tokens[i])
}

fn empty_config<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    cx.cop_config_str("EnforcedStyleForEmptyBrackets")
}
