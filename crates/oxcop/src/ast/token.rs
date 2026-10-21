//! Port of rubocop-ast's `lib/rubocop/ast/token.rb`: the predicates on
//! `processed_source.tokens` (the struct itself is [`Token`] in `lexer.rs`).
//! `line`/`column` need the source, so they take the processed source.

pub use crate::ast::lexer::Token;
use crate::processed_source::ProcessedSource;

const LEFT_PAREN_TYPES: &[&str] = &["tLPAREN", "tLPAREN2"];
const LEFT_CURLY_TYPES: &[&str] = &["tLCURLY", "tLAMBEG"];

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
}

impl Token {
    pub fn line(&self, ps: &ProcessedSource<'_>) -> usize {
        ps.line_of(self.range.begin_pos)
    }

    pub fn column(&self, ps: &ProcessedSource<'_>) -> usize {
        ps.column_of(self.range.begin_pos)
    }

    pub fn begin_pos(&self) -> usize {
        self.range.begin_pos
    }

    pub fn end_pos(&self) -> usize {
        self.range.end_pos
    }

    /// `space_after?`
    pub fn is_space_after(&self, ps: &ProcessedSource<'_>) -> bool {
        ps.buffer_source()
            .as_bytes()
            .get(self.range.end_pos)
            .is_some_and(|&b| is_space(b))
    }

    /// `space_before?`
    pub fn is_space_before(&self, ps: &ProcessedSource<'_>) -> bool {
        let position = self.range.begin_pos.saturating_sub(1);
        ps.buffer_source()
            .as_bytes()
            .get(position)
            .is_some_and(|&b| is_space(b))
    }

    pub fn is_comment(&self) -> bool {
        self.ty == "tCOMMENT"
    }

    pub fn is_semicolon(&self) -> bool {
        self.ty == "tSEMI"
    }

    pub fn is_left_array_bracket(&self) -> bool {
        self.ty == "tLBRACK"
    }

    pub fn is_left_ref_bracket(&self) -> bool {
        self.ty == "tLBRACK2"
    }

    pub fn is_left_bracket(&self) -> bool {
        matches!(self.ty, "tLBRACK" | "tLBRACK2")
    }

    pub fn is_right_bracket(&self) -> bool {
        self.ty == "tRBRACK"
    }

    pub fn is_left_brace(&self) -> bool {
        self.ty == "tLBRACE"
    }

    pub fn is_left_curly_brace(&self) -> bool {
        LEFT_CURLY_TYPES.contains(&self.ty)
    }

    pub fn is_right_curly_brace(&self) -> bool {
        self.ty == "tRCURLY"
    }

    pub fn is_left_parens(&self) -> bool {
        LEFT_PAREN_TYPES.contains(&self.ty)
    }

    pub fn is_right_parens(&self) -> bool {
        self.ty == "tRPAREN"
    }

    pub fn is_comma(&self) -> bool {
        self.ty == "tCOMMA"
    }

    pub fn is_dot(&self) -> bool {
        self.ty == "tDOT"
    }

    pub fn is_regexp_dots(&self) -> bool {
        matches!(self.ty, "tDOT2" | "tDOT3")
    }

    pub fn is_rescue_modifier(&self) -> bool {
        self.ty == "kRESCUE_MOD"
    }

    pub fn is_end(&self) -> bool {
        self.ty == "kEND"
    }

    pub fn is_equal_sign(&self) -> bool {
        matches!(self.ty, "tEQL" | "tOP_ASGN")
    }

    pub fn is_new_line(&self) -> bool {
        self.ty == "tNL"
    }
}
