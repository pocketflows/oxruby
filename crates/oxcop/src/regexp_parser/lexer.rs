//! Port of `regexp_parser`'s `Regexp::Lexer` (`lib/regexp_parser/lexer.rb`)
//! and the token normalization of `Regexp::Syntax::Base#normalize`.

use crate::regexp_parser::scanner::{self, ScanError, ScannedToken};

/// `Regexp::Token`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub ty: &'static str,
    pub token: String,
    pub text: String,
    pub ts: usize,
    pub te: usize,
    pub level: i64,
    pub set_level: i64,
    pub conditional_level: i64,
}

const OPENING_TOKENS: &[&str] = &[
    "capture",
    "passive",
    "lookahead",
    "nlookahead",
    "lookbehind",
    "nlookbehind",
    "atomic",
    "options",
    "options_switch",
    "named",
    "absence",
    "open",
];

/// `Regexp::Syntax::Base#normalize`.
fn normalize(ty: &'static str, token: String) -> String {
    match (ty, token.as_str()) {
        ("group", "named_ab" | "named_sq") => "named".to_owned(),
        ("backref", t) => match t {
            "name_ref_ab" | "name_ref_sq" => "name_ref",
            "name_call_ab" | "name_call_sq" => "name_call",
            "name_recursion_ref_ab" | "name_recursion_ref_sq" => "name_recursion_ref",
            "number_ref_ab" | "number_ref_sq" => "number_ref",
            "number_call_ab" | "number_call_sq" => "number_call",
            "number_rel_ref_ab" | "number_rel_ref_sq" => "number_rel_ref",
            "number_rel_call_ab" | "number_rel_call_sq" => "number_rel_call",
            "number_recursion_ref_ab" | "number_recursion_ref_sq" => "number_recursion_ref",
            other => other,
        }
        .to_owned(),
        _ => token,
    }
}

#[derive(Default)]
struct Lexer {
    tokens: Vec<Token>,
    prev_token: Option<Token>,
    nesting: i64,
    set_nesting: i64,
    conditional_nesting: i64,
    shift: usize,
}

/// `Regexp::Lexer.lex(input, options:)`.
pub fn lex(input: &str, extended: bool, binary: bool) -> Result<Vec<Token>, ScanError> {
    let scanned = scanner::scan(input, extended, binary)?;
    let mut lexer = Lexer::default();
    for ScannedToken {
        ty,
        token,
        text,
        ts,
        te,
    } in scanned
    {
        let token = normalize(ty, token);
        lexer.ascend(ty, &token);

        let mut last = lexer.prev_token.take();
        if ty == "quantifier"
            && let Some(prev) = &last
        {
            let broken = if prev.ty == "literal" {
                lexer.break_literal_run(prev)
            } else if prev.token == "codepoint_list" {
                lexer.break_codepoint_list(prev)
            } else {
                None
            };
            if let Some((shortened_run, quantified_node)) = broken {
                lexer.tokens.push(shortened_run);
                last = Some(quantified_node);
            }
        }

        let mut current = Token {
            ty,
            token,
            text,
            ts: ts + lexer.shift,
            te: te + lexer.shift,
            level: lexer.nesting,
            set_level: lexer.set_nesting,
            conditional_level: lexer.conditional_nesting,
        };

        if ty == "conditional" && matches!(current.token.as_str(), "condition" | "condition_close")
        {
            if let Some(last) = last {
                current = Token {
                    ty: "conditional",
                    token: "condition".to_owned(),
                    text: format!("{}{}", last.text, current.text),
                    ts: last.ts,
                    te: current.te,
                    level: lexer.nesting,
                    set_level: lexer.set_nesting,
                    conditional_level: lexer.conditional_nesting,
                };
            }
        } else if let Some(last) = last {
            lexer.tokens.push(last);
        }

        let descend_token = current.token.clone();
        lexer.prev_token = Some(current);
        lexer.descend(ty, &descend_token);
    }
    if let Some(prev) = lexer.prev_token.take() {
        lexer.tokens.push(prev);
    }
    Ok(lexer.tokens)
}

impl Lexer {
    fn ascend(&mut self, ty: &str, token: &str) {
        if token != "close" {
            return;
        }
        match ty {
            "group" | "assertion" => self.nesting -= 1,
            "set" => self.set_nesting -= 1,
            "conditional" => self.conditional_nesting -= 1,
            _ => {}
        }
    }

    fn descend(&mut self, ty: &str, token: &str) {
        if !OPENING_TOKENS.contains(&token) {
            return;
        }
        match ty {
            "group" | "assertion" => self.nesting += 1,
            "set" => self.set_nesting += 1,
            "conditional" => self.conditional_nesting += 1,
            _ => {}
        }
    }

    fn break_literal_run(&self, token: &Token) -> Option<(Token, Token)> {
        let last_char = token.text.chars().next_back()?;
        let lead = &token.text[..token.text.len() - last_char.len_utf8()];
        if lead.is_empty() {
            return None;
        }
        let lead_len = lead.chars().count();
        let token_1 = Token {
            ty: "literal",
            token: "literal".to_owned(),
            text: lead.to_owned(),
            ts: token.ts,
            te: token.te - 1,
            level: self.nesting,
            set_level: self.set_nesting,
            conditional_level: self.conditional_nesting,
        };
        let token_2 = Token {
            ty: "literal",
            token: "literal".to_owned(),
            text: last_char.to_string(),
            ts: token.ts + lead_len,
            te: token.te,
            level: self.nesting,
            set_level: self.set_nesting,
            conditional_level: self.conditional_nesting,
        };
        Some((token_1, token_2))
    }

    fn break_codepoint_list(&mut self, token: &Token) -> Option<(Token, Token)> {
        let (lead, tail) = token.text.rsplit_once(' ')?;
        if lead.is_empty() {
            return None;
        }
        let lead_len = lead.chars().count();
        let tail_len = tail.chars().count();
        let token_1 = Token {
            ty: "escape",
            token: "codepoint_list".to_owned(),
            text: format!("{lead}}}"),
            ts: token.ts,
            te: token.te - tail_len,
            level: self.nesting,
            set_level: self.set_nesting,
            conditional_level: self.conditional_nesting,
        };
        let token_2 = Token {
            ty: "escape",
            token: "codepoint_list".to_owned(),
            text: format!("\\u{{{tail}"),
            ts: token.ts + lead_len + 1,
            te: token.te + 3,
            level: self.nesting,
            set_level: self.set_nesting,
            conditional_level: self.conditional_nesting,
        };
        self.shift += 3;
        Some((token_1, token_2))
    }
}
