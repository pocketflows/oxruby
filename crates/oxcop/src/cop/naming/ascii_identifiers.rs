//! Port of `lib/rubocop/cop/naming/ascii_identifiers.rb`.

use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const IDENTIFIER_MSG: &str = "Use only ascii symbols in identifiers.";
const CONSTANT_MSG: &str = "Use only ascii symbols in constants.";

#[derive(Default)]
pub struct AsciiIdentifiers;

impl Cop for AsciiIdentifiers {
    fn name(&self) -> &'static str {
        "Naming/AsciiIdentifiers"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ascii_constants = cx.cop_config_bool("AsciiConstants", false);
        let offenses: Vec<(Range, &str)> = cx
            .processed_source
            .tokens()
            .iter()
            .filter(|token| {
                token.ty == "tIDENTIFIER" || (token.ty == "tCONSTANT" && ascii_constants)
            })
            .filter(|token| !token.text.is_ascii())
            .filter_map(|token| {
                let message = if token.ty == "tIDENTIFIER" {
                    IDENTIFIER_MSG
                } else {
                    CONSTANT_MSG
                };
                first_offense_range(token.range.begin_pos, &token.text).map(|r| (r, message))
            })
            .collect();
        for (range, message) in offenses {
            cx.add_offense(range, message);
        }
    }
}

/// The first run of non-ASCII characters in the identifier.
fn first_offense_range(begin_pos: usize, text: &str) -> Option<Range> {
    let (start, end) = regex!(r"[^[:ascii:]]+").find(text)?;
    Some(Range::new(begin_pos + start, begin_pos + end))
}
