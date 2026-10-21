//! Port of `lib/rubocop/cop/style/line_end_concatenation.rb`.

use crate::ast::lexer::Token;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const CONCAT_TOKEN_TYPES: &[&str] = &["tPLUS", "tLSHFT"];
const SIMPLE_STRING_TOKEN_TYPE: &str = "tSTRING";
const COMPLEX_STRING_BEGIN_TOKEN: &str = "tSTRING_BEG";
const COMPLEX_STRING_END_TOKEN: &str = "tSTRING_END";
const HIGH_PRECEDENCE_OP_TOKEN_TYPES: &[&str] = &["tSTAR2", "tPERCENT", "tDOT", "tLBRACK2"];
const QUOTE_DELIMITERS: &[&str] = &["'", "\""];

#[derive(Default)]
pub struct LineEndConcatenation;

impl Cop for LineEndConcatenation {
    fn name(&self) -> &'static str {
        "Style/LineEndConcatenation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/RedundantInterpolation"]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let tokens = ps.tokens();
        for index in 0..tokens.len() {
            let (Some(predecessor), Some(operator)) = (tokens.get(index), tokens.get(index + 1))
            else {
                continue;
            };
            let successor = tokens.get(index + 2);
            if !eligible_token_set(predecessor, operator, successor) {
                continue;
            }
            let successor = successor.unwrap();
            if ps.line_of(operator.range.begin_pos) == ps.line_of(successor.range.begin_pos) {
                continue;
            }
            let next_successor = token_after_last_string(tokens, successor, index);
            if next_successor.is_some_and(|t| HIGH_PRECEDENCE_OP_TOKEN_TYPES.contains(&t.ty)) {
                continue;
            }
            let message = format!(
                "Use `\\` instead of `{}` to concatenate multiline strings.",
                operator.text
            );
            let range = operator.range;
            let src = ps.buffer_source();
            cx.add_offense_with_correction(range, &message, |corrector| {
                autocorrect(src, corrector, range);
            });
        }
    }
}

fn autocorrect(src: &str, corrector: &mut Corrector, operator_range: Range) {
    // Include any trailing whitespace so we don't create a syntax error.
    let mut operator_range = range_with_surrounding_space(
        src,
        operator_range,
        SurroundingSpace {
            side: Side::Right,
            newlines: false,
            ..SurroundingSpace::default()
        },
    );
    let one_more_char = operator_range.resize(operator_range.len() + 1);
    // Don't create a double backslash at the end of the line, in case there
    // already was a backslash after the concatenation operator.
    if src
        .get(one_more_char.begin_pos..one_more_char.end_pos)
        .is_some_and(|s| s.ends_with('\\'))
    {
        operator_range = one_more_char;
    }
    corrector.replace(operator_range, "\\");
}

fn eligible_token_set(predecessor: &Token, operator: &Token, successor: Option<&Token>) -> bool {
    successor.is_some_and(standard_string_literal)
        && CONCAT_TOKEN_TYPES.contains(&operator.ty)
        && standard_string_literal(predecessor)
}

fn token_after_last_string<'t>(tokens: &'t [Token], successor: &Token, base_index: usize) -> Option<&'t Token> {
    let mut index = base_index + 3;
    if successor.ty == COMPLEX_STRING_BEGIN_TOKEN {
        let mut ends_to_find = 1;
        while ends_to_find > 0 {
            match tokens.get(index)?.ty {
                COMPLEX_STRING_BEGIN_TOKEN => ends_to_find += 1,
                COMPLEX_STRING_END_TOKEN => ends_to_find -= 1,
                _ => {}
            }
            index += 1;
        }
    }
    tokens.get(index)
}

fn standard_string_literal(token: &Token) -> bool {
    match token.ty {
        SIMPLE_STRING_TOKEN_TYPE => true,
        COMPLEX_STRING_BEGIN_TOKEN | COMPLEX_STRING_END_TOKEN => {
            QUOTE_DELIMITERS.contains(&token.text.as_str())
        }
        _ => false,
    }
}
