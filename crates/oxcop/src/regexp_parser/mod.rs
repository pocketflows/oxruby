//! Port of the `regexp_parser` gem (2.13.1), which RuboCop uses for
//! `RegexpNode#parsed_tree`: scanner, lexer, parser and expression tree.
//! The scanner's Unicode property tables (`properties/*.csv`) are copied
//! from the gem (MIT, (c) Ammar Ali).

pub mod expression;
pub mod lexer;
pub mod parser;
pub mod scanner;

pub use expression::{Class, Event, ExprId, Expression, Format, GroupKind, Quantifier, Tree};
pub use parser::ParseError;

/// `Regexp::Parser.parse(input, options:)`: `extended` is the `x` option
/// and `binary` the `n` option.
pub fn parse(input: &str, extended: bool, binary: bool) -> Result<Tree, ParseError> {
    parser::parse(input, extended, binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(tree: &Tree, id: ExprId, depth: usize, out: &mut Vec<String>) {
        let exp = tree.get(id);
        let q = exp.quantifier.as_ref().map(|q| {
            serde_json::json!([
                q.text,
                q.min,
                q.max,
                match q.mode {
                    expression::Mode::Greedy => "greedy",
                    expression::Mode::Reluctant => "reluctant",
                    expression::Mode::Possessive => "possessive",
                }
            ])
        });
        out.push(
            serde_json::json!([
                depth,
                exp.ty,
                exp.token,
                exp.text,
                tree.ts(id),
                tree.full_length(id),
                q
            ])
            .to_string(),
        );
        if !exp.is_terminal() {
            for &e in &exp.expressions {
                dump(tree, e, depth + 1, out);
            }
        }
    }

    pub fn dump_line(body: &str, extended: bool, binary: bool) -> String {
        match parse(body, extended, binary) {
            Ok(tree) => {
                let mut out = Vec::new();
                dump(&tree, Tree::ROOT, 0, &mut out);
                out.join(" ")
            }
            Err(_) => "ERR".to_owned(),
        }
    }

    /// Differential test against the Ruby gem: `REGEXP_CORPUS` is a file of
    /// `[source, x, n]` JSON lines, `REGEXP_OUT` where to write the dump.
    #[test]
    #[ignore]
    fn dump_corpus() {
        let corpus = std::fs::read_to_string(std::env::var("REGEXP_CORPUS").unwrap()).unwrap();
        let mut out = String::new();
        for line in corpus.lines() {
            let (body, x, n): (String, bool, bool) = serde_json::from_str(line).unwrap();
            // `options             out.push_str(&dump_line(&body, x, n)); NOENCODING` is 0, which Ruby treats as true: with any
            // `options:` the scanner works in binary mode.
            let _ = n;
            out.push_str(&dump_line(&body, x, true));
            out.push('\n');
        }
        std::fs::write(std::env::var("REGEXP_OUT").unwrap(), out).unwrap();
    }

    #[test]
    fn quantified_literal() {
        assert_eq!(
            dump_line("ab+", false, false),
            r#"[0,"expression","root","",0,3,null] [1,"literal","literal","a",0,1,null] [1,"literal","literal","b",1,2,["+",1,-1,"greedy"]]"#
        );
    }
}
