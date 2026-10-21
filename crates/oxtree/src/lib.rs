//! oxtree: a Rust port of the syntax_tree Ruby formatter (6.3.0) and its ERB
//! plugin (w_syntax_tree-erb 0.12.0).

// The formatter mirrors syntax_tree's Ruby control flow branch for branch.
#![allow(clippy::collapsible_match)]

pub mod format;
pub mod node;
pub mod parser;
pub mod prettier_print;

/// `SyntaxTree.format(source, maxwidth)`.
pub fn format(
    source: &str,
    maxwidth: usize,
    options: format::Options,
) -> Result<String, parser::ParseError> {
    let ast = parser::parse(source)?;
    Ok(format::format(&ast, maxwidth, 0, options))
}
