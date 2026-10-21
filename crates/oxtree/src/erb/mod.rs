//! Port of w_syntax_tree-erb 0.12.0 (`lib/syntax_tree/erb.rb`): formats ERB
//! templates, with the Ruby inside tags formatted by syntax_tree.

pub mod format;
pub mod parser;

use crate::parser::ParseError;

/// `SyntaxTree::ERB::MAX_WIDTH`
pub const MAX_WIDTH: usize = 80;

/// `SyntaxTree::ERB.parse`
pub fn parse(source: &str) -> Result<parser::Document, ParseError> {
    parser::Parser::new(source).parse()
}

/// `SyntaxTree::ERB.format(source, maxwidth)`
pub fn format(source: &str, maxwidth: usize) -> Result<String, ParseError> {
    let document = parse(source)?;
    Ok(format::format_document(&document, maxwidth, None))
}
