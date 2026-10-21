//! Port of `Prism::Translation::Parser::Lexer` (prism 1.9): converts Prism's
//! token stream into the `parser` gem's tokens, which RuboCop exposes as
//! `processed_source.tokens` (`RuboCop::AST::Token`).

use crate::processed_source::Range;

/// A `RuboCop::AST::Token`.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The parser token type, e.g. `tIDENTIFIER` or `kDO`.
    pub ty: &'static str,
    /// The token's value as `Token#text` (`value.to_s`).
    pub text: String,
    pub range: Range,
}

/// Prism's `pm_token_type_t` names, indexed by value.
const PRISM_TOKEN_NAMES: &[&str] = &[
    "",
    "EOF",
    "BRACE_RIGHT",
    "COMMA",
    "EMBEXPR_END",
    "KEYWORD_DO",
    "KEYWORD_ELSE",
    "KEYWORD_ELSIF",
    "KEYWORD_END",
    "KEYWORD_ENSURE",
    "KEYWORD_IN",
    "KEYWORD_RESCUE",
    "KEYWORD_THEN",
    "KEYWORD_WHEN",
    "NEWLINE",
    "PARENTHESIS_RIGHT",
    "PIPE",
    "SEMICOLON",
    "AMPERSAND",
    "AMPERSAND_AMPERSAND",
    "AMPERSAND_AMPERSAND_EQUAL",
    "AMPERSAND_DOT",
    "AMPERSAND_EQUAL",
    "BACKTICK",
    "BACK_REFERENCE",
    "BANG",
    "BANG_EQUAL",
    "BANG_TILDE",
    "BRACE_LEFT",
    "BRACKET_LEFT",
    "BRACKET_LEFT_ARRAY",
    "BRACKET_LEFT_RIGHT",
    "BRACKET_LEFT_RIGHT_EQUAL",
    "BRACKET_RIGHT",
    "CARET",
    "CARET_EQUAL",
    "CHARACTER_LITERAL",
    "CLASS_VARIABLE",
    "COLON",
    "COLON_COLON",
    "COMMENT",
    "CONSTANT",
    "DOT",
    "DOT_DOT",
    "DOT_DOT_DOT",
    "EMBDOC_BEGIN",
    "EMBDOC_END",
    "EMBDOC_LINE",
    "EMBEXPR_BEGIN",
    "EMBVAR",
    "EQUAL",
    "EQUAL_EQUAL",
    "EQUAL_EQUAL_EQUAL",
    "EQUAL_GREATER",
    "EQUAL_TILDE",
    "FLOAT",
    "FLOAT_IMAGINARY",
    "FLOAT_RATIONAL",
    "FLOAT_RATIONAL_IMAGINARY",
    "GLOBAL_VARIABLE",
    "GREATER",
    "GREATER_EQUAL",
    "GREATER_GREATER",
    "GREATER_GREATER_EQUAL",
    "HEREDOC_END",
    "HEREDOC_START",
    "IDENTIFIER",
    "IGNORED_NEWLINE",
    "INSTANCE_VARIABLE",
    "INTEGER",
    "INTEGER_IMAGINARY",
    "INTEGER_RATIONAL",
    "INTEGER_RATIONAL_IMAGINARY",
    "KEYWORD_ALIAS",
    "KEYWORD_AND",
    "KEYWORD_BEGIN",
    "KEYWORD_BEGIN_UPCASE",
    "KEYWORD_BREAK",
    "KEYWORD_CASE",
    "KEYWORD_CLASS",
    "KEYWORD_DEF",
    "KEYWORD_DEFINED",
    "KEYWORD_DO_LOOP",
    "KEYWORD_END_UPCASE",
    "KEYWORD_FALSE",
    "KEYWORD_FOR",
    "KEYWORD_IF",
    "KEYWORD_IF_MODIFIER",
    "KEYWORD_MODULE",
    "KEYWORD_NEXT",
    "KEYWORD_NIL",
    "KEYWORD_NOT",
    "KEYWORD_OR",
    "KEYWORD_REDO",
    "KEYWORD_RESCUE_MODIFIER",
    "KEYWORD_RETRY",
    "KEYWORD_RETURN",
    "KEYWORD_SELF",
    "KEYWORD_SUPER",
    "KEYWORD_TRUE",
    "KEYWORD_UNDEF",
    "KEYWORD_UNLESS",
    "KEYWORD_UNLESS_MODIFIER",
    "KEYWORD_UNTIL",
    "KEYWORD_UNTIL_MODIFIER",
    "KEYWORD_WHILE",
    "KEYWORD_WHILE_MODIFIER",
    "KEYWORD_YIELD",
    "KEYWORD___ENCODING__",
    "KEYWORD___FILE__",
    "KEYWORD___LINE__",
    "LABEL",
    "LABEL_END",
    "LAMBDA_BEGIN",
    "LESS",
    "LESS_EQUAL",
    "LESS_EQUAL_GREATER",
    "LESS_LESS",
    "LESS_LESS_EQUAL",
    "METHOD_NAME",
    "MINUS",
    "MINUS_EQUAL",
    "MINUS_GREATER",
    "NUMBERED_REFERENCE",
    "PARENTHESIS_LEFT",
    "PARENTHESIS_LEFT_PARENTHESES",
    "PERCENT",
    "PERCENT_EQUAL",
    "PERCENT_LOWER_I",
    "PERCENT_LOWER_W",
    "PERCENT_LOWER_X",
    "PERCENT_UPPER_I",
    "PERCENT_UPPER_W",
    "PIPE_EQUAL",
    "PIPE_PIPE",
    "PIPE_PIPE_EQUAL",
    "PLUS",
    "PLUS_EQUAL",
    "QUESTION_MARK",
    "REGEXP_BEGIN",
    "REGEXP_END",
    "SLASH",
    "SLASH_EQUAL",
    "STAR",
    "STAR_EQUAL",
    "STAR_STAR",
    "STAR_STAR_EQUAL",
    "STRING_BEGIN",
    "STRING_CONTENT",
    "STRING_END",
    "SYMBOL_BEGIN",
    "TILDE",
    "UAMPERSAND",
    "UCOLON_COLON",
    "UDOT_DOT",
    "UDOT_DOT_DOT",
    "UMINUS",
    "UMINUS_NUM",
    "UPLUS",
    "USTAR",
    "USTAR_STAR",
    "WORDS_SEP",
    "__END__",
    "MISSING",
    "NOT_PROVIDED",
];

/// `TYPES`: the direct translation between the two lexers.
fn translate_type(prism: &str) -> Option<&'static str> {
    Some(match prism {
        "AMPERSAND" => "tAMPER2",
        "AMPERSAND_AMPERSAND" => "tANDOP",
        "AMPERSAND_AMPERSAND_EQUAL" => "tOP_ASGN",
        "AMPERSAND_DOT" => "tANDDOT",
        "AMPERSAND_EQUAL" => "tOP_ASGN",
        "BACK_REFERENCE" => "tBACK_REF",
        "BACKTICK" => "tXSTRING_BEG",
        "BANG" => "tBANG",
        "BANG_EQUAL" => "tNEQ",
        "BANG_TILDE" => "tNMATCH",
        "BRACE_LEFT" => "tLCURLY",
        "BRACE_RIGHT" => "tRCURLY",
        "BRACKET_LEFT" => "tLBRACK2",
        "BRACKET_LEFT_ARRAY" => "tLBRACK",
        "BRACKET_LEFT_RIGHT" => "tAREF",
        "BRACKET_LEFT_RIGHT_EQUAL" => "tASET",
        "BRACKET_RIGHT" => "tRBRACK",
        "CARET" => "tCARET",
        "CARET_EQUAL" => "tOP_ASGN",
        "CHARACTER_LITERAL" => "tCHARACTER",
        "CLASS_VARIABLE" => "tCVAR",
        "COLON" => "tCOLON",
        "COLON_COLON" => "tCOLON2",
        "COMMA" => "tCOMMA",
        "COMMENT" => "tCOMMENT",
        "CONSTANT" => "tCONSTANT",
        "DOT" => "tDOT",
        "DOT_DOT" => "tDOT2",
        "DOT_DOT_DOT" => "tDOT3",
        "EMBDOC_BEGIN" => "tCOMMENT",
        "EMBEXPR_BEGIN" => "tSTRING_DBEG",
        "EMBEXPR_END" => "tSTRING_DEND",
        "EMBVAR" => "tSTRING_DVAR",
        "EQUAL" => "tEQL",
        "EQUAL_EQUAL" => "tEQ",
        "EQUAL_EQUAL_EQUAL" => "tEQQ",
        "EQUAL_GREATER" => "tASSOC",
        "EQUAL_TILDE" => "tMATCH",
        "FLOAT" => "tFLOAT",
        "FLOAT_IMAGINARY" => "tIMAGINARY",
        "FLOAT_RATIONAL" => "tRATIONAL",
        "FLOAT_RATIONAL_IMAGINARY" => "tIMAGINARY",
        "GLOBAL_VARIABLE" => "tGVAR",
        "GREATER" => "tGT",
        "GREATER_EQUAL" => "tGEQ",
        "GREATER_GREATER" => "tRSHFT",
        "GREATER_GREATER_EQUAL" => "tOP_ASGN",
        "HEREDOC_START" => "tSTRING_BEG",
        "HEREDOC_END" => "tSTRING_END",
        "IDENTIFIER" => "tIDENTIFIER",
        "INSTANCE_VARIABLE" => "tIVAR",
        "INTEGER" => "tINTEGER",
        "INTEGER_IMAGINARY" => "tIMAGINARY",
        "INTEGER_RATIONAL" => "tRATIONAL",
        "INTEGER_RATIONAL_IMAGINARY" => "tIMAGINARY",
        "KEYWORD_ALIAS" => "kALIAS",
        "KEYWORD_AND" => "kAND",
        "KEYWORD_BEGIN" => "kBEGIN",
        "KEYWORD_BEGIN_UPCASE" => "klBEGIN",
        "KEYWORD_BREAK" => "kBREAK",
        "KEYWORD_CASE" => "kCASE",
        "KEYWORD_CLASS" => "kCLASS",
        "KEYWORD_DEF" => "kDEF",
        "KEYWORD_DEFINED" => "kDEFINED",
        "KEYWORD_DO" => "kDO",
        "KEYWORD_DO_LOOP" => "kDO_COND",
        "KEYWORD_END" => "kEND",
        "KEYWORD_END_UPCASE" => "klEND",
        "KEYWORD_ENSURE" => "kENSURE",
        "KEYWORD_ELSE" => "kELSE",
        "KEYWORD_ELSIF" => "kELSIF",
        "KEYWORD_FALSE" => "kFALSE",
        "KEYWORD_FOR" => "kFOR",
        "KEYWORD_IF" => "kIF",
        "KEYWORD_IF_MODIFIER" => "kIF_MOD",
        "KEYWORD_IN" => "kIN",
        "KEYWORD_MODULE" => "kMODULE",
        "KEYWORD_NEXT" => "kNEXT",
        "KEYWORD_NIL" => "kNIL",
        "KEYWORD_NOT" => "kNOT",
        "KEYWORD_OR" => "kOR",
        "KEYWORD_REDO" => "kREDO",
        "KEYWORD_RESCUE" => "kRESCUE",
        "KEYWORD_RESCUE_MODIFIER" => "kRESCUE_MOD",
        "KEYWORD_RETRY" => "kRETRY",
        "KEYWORD_RETURN" => "kRETURN",
        "KEYWORD_SELF" => "kSELF",
        "KEYWORD_SUPER" => "kSUPER",
        "KEYWORD_THEN" => "kTHEN",
        "KEYWORD_TRUE" => "kTRUE",
        "KEYWORD_UNDEF" => "kUNDEF",
        "KEYWORD_UNLESS" => "kUNLESS",
        "KEYWORD_UNLESS_MODIFIER" => "kUNLESS_MOD",
        "KEYWORD_UNTIL" => "kUNTIL",
        "KEYWORD_UNTIL_MODIFIER" => "kUNTIL_MOD",
        "KEYWORD_WHEN" => "kWHEN",
        "KEYWORD_WHILE" => "kWHILE",
        "KEYWORD_WHILE_MODIFIER" => "kWHILE_MOD",
        "KEYWORD_YIELD" => "kYIELD",
        "KEYWORD___ENCODING__" => "k__ENCODING__",
        "KEYWORD___FILE__" => "k__FILE__",
        "KEYWORD___LINE__" => "k__LINE__",
        "LABEL" => "tLABEL",
        "LABEL_END" => "tLABEL_END",
        "LAMBDA_BEGIN" => "tLAMBEG",
        "LESS" => "tLT",
        "LESS_EQUAL" => "tLEQ",
        "LESS_EQUAL_GREATER" => "tCMP",
        "LESS_LESS" => "tLSHFT",
        "LESS_LESS_EQUAL" => "tOP_ASGN",
        "METHOD_NAME" => "tFID",
        "MINUS" => "tMINUS",
        "MINUS_EQUAL" => "tOP_ASGN",
        "MINUS_GREATER" => "tLAMBDA",
        "NEWLINE" => "tNL",
        "NUMBERED_REFERENCE" => "tNTH_REF",
        "PARENTHESIS_LEFT" => "tLPAREN2",
        "PARENTHESIS_LEFT_PARENTHESES" => "tLPAREN_ARG",
        "PARENTHESIS_RIGHT" => "tRPAREN",
        "PERCENT" => "tPERCENT",
        "PERCENT_EQUAL" => "tOP_ASGN",
        "PERCENT_LOWER_I" => "tQSYMBOLS_BEG",
        "PERCENT_LOWER_W" => "tQWORDS_BEG",
        "PERCENT_UPPER_I" => "tSYMBOLS_BEG",
        "PERCENT_UPPER_W" => "tWORDS_BEG",
        "PERCENT_LOWER_X" => "tXSTRING_BEG",
        "PLUS" => "tPLUS",
        "PLUS_EQUAL" => "tOP_ASGN",
        "PIPE_EQUAL" => "tOP_ASGN",
        "PIPE" => "tPIPE",
        "PIPE_PIPE" => "tOROP",
        "PIPE_PIPE_EQUAL" => "tOP_ASGN",
        "QUESTION_MARK" => "tEH",
        "REGEXP_BEGIN" => "tREGEXP_BEG",
        "REGEXP_END" => "tSTRING_END",
        "SEMICOLON" => "tSEMI",
        "SLASH" => "tDIVIDE",
        "SLASH_EQUAL" => "tOP_ASGN",
        "STAR" => "tSTAR2",
        "STAR_EQUAL" => "tOP_ASGN",
        "STAR_STAR" => "tPOW",
        "STAR_STAR_EQUAL" => "tOP_ASGN",
        "STRING_BEGIN" => "tSTRING_BEG",
        "STRING_CONTENT" => "tSTRING_CONTENT",
        "STRING_END" => "tSTRING_END",
        "SYMBOL_BEGIN" => "tSYMBEG",
        "TILDE" => "tTILDE",
        "UAMPERSAND" => "tAMPER",
        "UCOLON_COLON" => "tCOLON3",
        "UDOT_DOT" => "tBDOT2",
        "UDOT_DOT_DOT" => "tBDOT3",
        "UMINUS" => "tUMINUS",
        "UMINUS_NUM" => "tUNARY_NUM",
        "UPLUS" => "tUPLUS",
        "USTAR" => "tSTAR",
        "USTAR_STAR" => "tDSTAR",
        "WORDS_SEP" => "tSPACE",
        _ => return None,
    })
}

const EXPR_BEG: u32 = 0x1;
const EXPR_LABEL: u32 = 0x400;

const LAMBDA_TOKEN_TYPES: &[&str] = &["kDO_LAMBDA", "tLAMBDA", "tLAMBEG"];

const LPAREN_CONVERSION_TOKEN_TYPES: &[&str] = &[
    "kBREAK",
    "tCARET",
    "kCASE",
    "tDIVIDE",
    "kFOR",
    "kIF",
    "kNEXT",
    "kRETURN",
    "kUNTIL",
    "kWHILE",
    "tAMPER",
    "tANDOP",
    "tBANG",
    "tCOMMA",
    "tDOT2",
    "tDOT3",
    "tEQL",
    "tLPAREN",
    "tLPAREN2",
    "tLPAREN_ARG",
    "tLSHFT",
    "tNL",
    "tOP_ASGN",
    "tOROP",
    "tPIPE",
    "tSEMI",
    "tSTRING_DBEG",
    "tUMINUS",
    "tUPLUS",
    "tLCURLY",
];

const COMMENT_CONTINUATION_TYPES: &[&str] = &["COMMENT", "AMPERSAND_DOT", "DOT"];

#[derive(Clone, Copy)]
struct Lexed<'s> {
    ty: &'static str,
    start: usize,
    end: usize,
    state: u32,
    value: &'s [u8],
}

struct HeredocData {
    identifier: Vec<u8>,
    common_whitespace: usize,
}

fn s(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub struct Lexer<'s> {
    lexed: Vec<Lexed<'s>>,
    line_starts: Vec<usize>,
}

impl<'s> Lexer<'s> {
    pub fn new(source: &'s str, tokens: &[crate::prism::RawToken]) -> Self {
        let source = source.as_bytes();
        let lexed = tokens
            .iter()
            .map(|t| Lexed {
                ty: PRISM_TOKEN_NAMES
                    .get(t.type_ as usize)
                    .copied()
                    .unwrap_or("MISSING"),
                start: t.start,
                end: t.end,
                state: t.state,
                value: &source[t.start.min(source.len())..t.end.min(source.len())],
            })
            .collect();
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self { lexed, line_starts }
    }

    fn start_line(&self, token: &Lexed<'_>) -> usize {
        match self.line_starts.binary_search(&token.start) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    fn start_column(&self, token: &Lexed<'_>) -> usize {
        token.start - self.line_starts[self.start_line(token)]
    }

    #[allow(clippy::too_many_lines)]
    pub fn tokens(&self) -> Vec<Token> {
        let lexed = &self.lexed;
        let mut tokens: Vec<Token> = Vec::new();
        let mut index = 0;
        let length = lexed.len();
        let mut heredoc_stack: Vec<HeredocData> = Vec::new();
        let mut quote_stack: Vec<Vec<u8>> = Vec::new();
        let mut comment_newline_location: Option<Range> = None;

        let push =
            |tokens: &mut Vec<Token>, ty: &'static str, text: Option<String>, range: Range| {
                let text = match (ty, text) {
                    (_, None) => String::new(),
                    ("tINTEGER" | "tNTH_REF", Some(text)) => parse_integer(&text),
                    ("tFLOAT", Some(text)) => parse_float(&text),
                    (_, Some(text)) => text,
                };
                tokens.push(Token { ty, text, range });
            };

        while index < length {
            let mut token = lexed[index];
            let state = token.state;
            index += 1;
            if matches!(token.ty, "IGNORED_NEWLINE" | "__END__" | "EOF") {
                continue;
            }
            let Some(mut ty) = translate_type(token.ty) else {
                continue;
            };
            let mut value: Option<Vec<u8>> = Some(token.value.to_vec());
            let mut location = Range::new(token.start, token.end);

            match ty {
                "kDO" => {
                    let nearest = tokens
                        .iter()
                        .rev()
                        .find(|t| LAMBDA_TOKEN_TYPES.contains(&t.ty));
                    if nearest.is_some_and(|t| t.ty == "tLAMBDA") {
                        ty = "kDO_LAMBDA";
                    }
                }
                "tCHARACTER" => {
                    let v = value.take().unwrap();
                    let v = v.strip_prefix(b"?").unwrap_or(&v).to_vec();
                    value = Some(self.unescape_string(&v, b"?"));
                }
                "tCOMMENT" => {
                    if token.ty == "EMBDOC_BEGIN" {
                        let mut v = token.value.to_vec();
                        let mut next_token = lexed.get(index).copied();
                        while !next_token.is_some_and(|t| t.ty == "EMBDOC_END")
                            && index < length - 1
                        {
                            if let Some(t) = next_token {
                                v.extend_from_slice(t.value);
                            }
                            index += 1;
                            next_token = lexed.get(index).copied();
                        }
                        if let Some(t) = next_token {
                            v.extend_from_slice(t.value);
                            location = Range::new(token.start, t.end);
                        }
                        value = Some(v);
                        index += 1;
                    } else {
                        let mut v = token.value.to_vec();
                        let is_at_eol = !v.ends_with(b"\n");
                        if !is_at_eol {
                            v.pop();
                            if v.ends_with(b"\r") {
                                v.pop();
                            }
                        }
                        location = Range::new(
                            token.start,
                            if is_at_eol { token.end } else { token.end - 1 },
                        );
                        let prev_token = index.checked_sub(2).map(|i| lexed[i]);
                        let next_token = lexed.get(index).copied();
                        let is_inline_comment = prev_token
                            .is_some_and(|p| self.start_line(&p) == self.start_line(&token));
                        let next_is_continuation =
                            next_token.is_some_and(|n| COMMENT_CONTINUATION_TYPES.contains(&n.ty));
                        if is_inline_comment && !is_at_eol && !next_is_continuation {
                            push(&mut tokens, "tCOMMENT", Some(s(&v)), location);
                            push(
                                &mut tokens,
                                "tNL",
                                None,
                                Range::new(token.end - 1, token.end),
                            );
                            continue;
                        } else if is_inline_comment && next_token.is_some_and(|n| n.ty == "COMMENT")
                        {
                            comment_newline_location = Some(Range::new(token.end - 1, token.end));
                        } else if let Some(nl) = comment_newline_location
                            && !next_is_continuation
                        {
                            push(&mut tokens, "tCOMMENT", Some(s(&v)), location);
                            push(&mut tokens, "tNL", None, nl);
                            comment_newline_location = None;
                            continue;
                        }
                        value = Some(v);
                    }
                }
                "tNL" => {
                    if lexed.get(index).is_some_and(|n| n.ty == "COMMENT") {
                        comment_newline_location = Some(location);
                        continue;
                    }
                    value = None;
                }
                "tINTEGER" => {
                    if token.value.starts_with(b"+") {
                        push(
                            &mut tokens,
                            "tUNARY_NUM",
                            Some("+".to_owned()),
                            Range::new(token.start, token.start + 1),
                        );
                        location = Range::new(token.start + 1, token.end);
                    }
                }
                "tLABEL" | "tLABEL_END" => {
                    let mut v = value.take().unwrap();
                    if v.ends_with(b":") {
                        v.pop();
                    }
                    value = Some(v);
                }
                "tLCURLY" => {
                    if state == EXPR_BEG | EXPR_LABEL {
                        ty = "tLBRACE";
                    }
                }
                "tLPAREN2" => {
                    if tokens.is_empty()
                        || LPAREN_CONVERSION_TOKEN_TYPES.contains(&tokens.last().unwrap().ty)
                    {
                        ty = "tLPAREN";
                    }
                }
                "tNTH_REF" => {
                    let v = value.take().unwrap();
                    value = Some(v.strip_prefix(b"$").unwrap_or(&v).to_vec());
                }
                "tOP_ASGN" => {
                    let mut v = value.take().unwrap();
                    if v.ends_with(b"=") {
                        v.pop();
                    }
                    value = Some(v);
                }
                "tSPACE" => {
                    location = Range::new(
                        token.start,
                        token.start + percent_array_leading_whitespace(token.value),
                    );
                    value = None;
                }
                "tSTRING_BEG" => {
                    let next_token = lexed.get(index).copied();
                    let next_next_token = lexed.get(index + 1).copied();
                    let v = token.value;
                    let basic_quotes = v == b"\"" || v == b"'";
                    if basic_quotes && next_token.is_some_and(|n| n.ty == "STRING_END") {
                        let next = next_token.unwrap();
                        ty = "tSTRING";
                        value = Some(Vec::new());
                        location = Range::new(token.start.min(next.start), token.end.max(next.end));
                        index += 1;
                    } else if v.starts_with(b"'") || v.starts_with(b"\"") || v.starts_with(b"%") {
                        if let (Some(next), Some(next_next)) = (next_token, next_next_token)
                            && next.ty == "STRING_CONTENT"
                            && next_next.ty == "STRING_END"
                        {
                            let string_value = next.value;
                            if simplify_string(string_value, v) {
                                let value = if percent_array(v) {
                                    percent_array_unescape(string_value)
                                } else {
                                    self.unescape_string(string_value, v)
                                };
                                index += 2;
                                push(
                                    &mut tokens,
                                    "tSTRING",
                                    Some(s(&value)),
                                    Range::new(token.start, next_next.end),
                                );
                                continue;
                            }
                        }
                        quote_stack.push(v.to_vec());
                    } else if token.ty == "HEREDOC_START" {
                        let squiggly_or_dash = v.get(2).is_some_and(|&c| c == b'-' || c == b'~');
                        let quote = if squiggly_or_dash { v.get(3) } else { v.get(2) }.copied();
                        let heredoc_type = if squiggly_or_dash {
                            v.get(2).copied()
                        } else {
                            None
                        };
                        let mut heredoc = HeredocData {
                            identifier: heredoc_identifier(v),
                            common_whitespace: 0,
                        };
                        if quote == Some(b'`') {
                            ty = "tXSTRING_BEG";
                        }
                        if heredoc_type == Some(b'~') || heredoc_type == Some(b'`') {
                            heredoc.common_whitespace = self.calculate_heredoc_whitespace(index);
                        }
                        let quoted = match quote {
                            Some(q @ (b'\'' | b'"' | b'`')) => vec![b'<', b'<', q],
                            _ => b"<<\"".to_vec(),
                        };
                        value = Some(quoted.clone());
                        heredoc_stack.push(heredoc);
                        quote_stack.push(quoted);
                    }
                }
                "tSTRING_CONTENT" => {
                    let quote = quote_stack.last().cloned().unwrap_or_default();
                    let is_percent_array = percent_array(&quote);
                    let lines = lines(token.value);
                    if lines.len() == 1 {
                        let mut current_string: Vec<u8> = Vec::new();
                        let mut current_length = 0;
                        let start_offset = token.start;
                        loop {
                            if token.ty != "STRING_CONTENT" {
                                break;
                            }
                            current_length += token.value.len();
                            let prev_token = index.checked_sub(2).map(|i| lexed[i]);
                            let is_first_token_on_line = prev_token
                                .is_some_and(|p| self.start_line(&token) != self.start_line(&p));
                            let not_nested = heredoc_stack.len() == 1;
                            let mut v: Vec<u8> = token.value.to_vec();
                            if is_percent_array {
                                v = percent_array_unescape(token.value);
                            } else if is_first_token_on_line
                                && not_nested
                                && heredoc_stack
                                    .last()
                                    .is_some_and(|h| h.common_whitespace > 0)
                            {
                                v = trim_heredoc_whitespace(
                                    token.value,
                                    heredoc_stack.last().unwrap().common_whitespace,
                                );
                            }
                            current_string.extend(self.unescape_string(&v, &quote));
                            let relevant_backslash_count =
                                if quote.starts_with(b"%W") || quote.starts_with(b"%I") {
                                    0
                                } else {
                                    backslashes_before_newline(token.value)
                                };
                            if relevant_backslash_count % 2 == 0 || !interpolation(&quote) {
                                push(
                                    &mut tokens,
                                    "tSTRING_CONTENT",
                                    Some(s(&current_string)),
                                    Range::new(start_offset, start_offset + current_length),
                                );
                                break;
                            }
                            match lexed.get(index) {
                                Some(next) => token = *next,
                                None => break,
                            }
                            index += 1;
                        }
                    } else {
                        let mut current_line: Vec<u8> = Vec::new();
                        let mut adjustment = 0;
                        let mut start_offset = token.start;
                        let count = lines.len();
                        for (i, line) in lines.iter().enumerate() {
                            let chomped_line = chomp(line);
                            let backslash_count = chomped_line
                                .iter()
                                .rev()
                                .take_while(|&&b| b == b'\\')
                                .count();
                            let is_interpolation = interpolation(&quote);
                            let emit = if backslash_count % 2 == 1
                                && (is_interpolation || is_percent_array)
                            {
                                if is_percent_array {
                                    current_line.extend(percent_array_unescape(line));
                                    adjustment += 1;
                                } else {
                                    current_line
                                        .extend_from_slice(&chomped_line[..chomped_line.len() - 1]);
                                    adjustment += 2;
                                }
                                i == count - 1
                            } else {
                                current_line.extend_from_slice(line);
                                true
                            };
                            if emit {
                                let end_offset = start_offset + current_line.len() + adjustment;
                                let unescaped = self.unescape_string(&current_line, &quote);
                                push(
                                    &mut tokens,
                                    "tSTRING_CONTENT",
                                    Some(s(&unescaped)),
                                    Range::new(start_offset, end_offset),
                                );
                                start_offset = end_offset;
                                current_line.clear();
                                adjustment = 0;
                            }
                        }
                    }
                    continue;
                }
                "tSTRING_DVAR" => value = None,
                "tSTRING_END" => {
                    let v = token.value;
                    if token.ty == "HEREDOC_END" && v.ends_with(b"\n") {
                        let newline_length = if v.ends_with(b"\r\n") { 2 } else { 1 };
                        value = heredoc_stack.pop().map(|h| h.identifier);
                        location = Range::new(token.start, token.end - newline_length);
                    } else if token.ty == "REGEXP_END" {
                        value = Some(v[..1.min(v.len())].to_vec());
                        location = Range::new(token.start, token.start + 1);
                    }
                    if quote_stack.pop().is_some_and(|q| percent_array(&q)) {
                        let prev_token = index.checked_sub(2).map(|i| lexed[i]);
                        let empty = prev_token.is_some_and(|p| {
                            matches!(
                                p.ty,
                                "PERCENT_LOWER_I"
                                    | "PERCENT_LOWER_W"
                                    | "PERCENT_UPPER_I"
                                    | "PERCENT_UPPER_W"
                            )
                        });
                        let ends_with_whitespace = prev_token.is_some_and(|p| p.ty == "WORDS_SEP");
                        if !empty && !ends_with_whitespace {
                            push(
                                &mut tokens,
                                "tSPACE",
                                None,
                                Range::new(token.start, token.start),
                            );
                        }
                    }
                }
                "tSYMBEG" => {
                    if let Some(next) = lexed.get(index).copied()
                        && !matches!(
                            next.ty,
                            "STRING_CONTENT" | "EMBEXPR_BEGIN" | "EMBVAR" | "STRING_END"
                        )
                    {
                        ty = "tSYMBOL";
                        let v = match next.value {
                            b"~@" => b"~".as_slice(),
                            b"!@" => b"!".as_slice(),
                            other => other,
                        };
                        value = Some(v.to_vec());
                        location = Range::new(token.start.min(next.start), token.end.max(next.end));
                        index += 1;
                    } else {
                        quote_stack.push(token.value.to_vec());
                    }
                }
                "tFID" => {
                    if tokens.last().is_some_and(|t| t.ty == "kDEF") {
                        ty = "tIDENTIFIER";
                    }
                }
                "tXSTRING_BEG" => {
                    if let Some(next) = lexed.get(index)
                        && !matches!(next.ty, "STRING_CONTENT" | "STRING_END" | "EMBEXPR_BEGIN")
                    {
                        ty = "tBACK_REF2";
                    }
                    quote_stack.push(token.value.to_vec());
                }
                "tSYMBOLS_BEG" | "tQSYMBOLS_BEG" | "tWORDS_BEG" | "tQWORDS_BEG" => {
                    if lexed.get(index).is_some_and(|n| n.ty == "WORDS_SEP") {
                        index += 1;
                    }
                    quote_stack.push(token.value.to_vec());
                }
                "tREGEXP_BEG" => quote_stack.push(token.value.to_vec()),
                _ => {}
            }

            push(&mut tokens, ty, value.map(|v| s(&v)), location);

            if token.ty == "REGEXP_END" {
                push(
                    &mut tokens,
                    "tREGEXP_OPT",
                    Some(s(token.value.get(1..).unwrap_or_default())),
                    Range::new(token.start + 1, token.end),
                );
            }
        }

        tokens
    }

    /// Wonky heredoc tab/spaces rules.
    fn calculate_heredoc_whitespace(&self, heredoc_token_index: usize) -> usize {
        let lexed = &self.lexed;
        let mut next_token_index = heredoc_token_index;
        let mut nesting_level: i64 = 0;
        let mut previous_line: Option<usize> = None;
        let mut result = usize::MAX;

        while let Some(next_token) = lexed.get(next_token_index).copied() {
            next_token_index += 1;
            let next_next_token = lexed.get(next_token_index).copied();
            let first_token_on_line = self.start_column(&next_token) == 0;

            if next_token.ty == "HEREDOC_START" || next_token.ty == "EMBEXPR_BEGIN" {
                if nesting_level == 0 && first_token_on_line {
                    result = 0;
                }
                nesting_level += 1;
            } else if next_token.ty == "HEREDOC_END" || next_token.ty == "EMBEXPR_END" {
                nesting_level -= 1;
                if nesting_level == -1 {
                    break;
                }
            } else if next_token.ty == "STRING_CONTENT" && nesting_level == 0 && first_token_on_line
            {
                let mut common_whitespace = 0;
                let leading: Vec<u8> = next_token
                    .value
                    .iter()
                    .copied()
                    .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
                    .collect();
                for c in leading {
                    if c == b'\t' {
                        common_whitespace = (common_whitespace / 8 + 1) * 8;
                    } else {
                        common_whitespace += 1;
                    }
                }
                let line = self.start_line(&next_token);
                let is_first_token_on_line = previous_line != Some(line);
                let value_length = String::from_utf8_lossy(next_token.value).chars().count();
                let whitespace_only = common_whitespace == value_length
                    && next_next_token.is_none_or(|n| self.start_line(&n) != line);
                if is_first_token_on_line && !whitespace_only && common_whitespace < result {
                    result = common_whitespace;
                    previous_line = Some(line);
                }
            }
        }
        result
    }

    /// Apply Ruby string escaping rules.
    fn unescape_string(&self, string: &[u8], quote: &[u8]) -> Vec<u8> {
        if quote == b"<<'" {
            return string.to_vec();
        }
        if !string.contains(&b'\\') {
            return string.to_vec();
        }
        let delimiter = *quote.last().unwrap_or(&b'"');
        if regexp(quote) {
            const REGEXP_META_CHARACTERS: &[u8] = b"\\$()*+.<>?[]^{|}";
            if REGEXP_META_CHARACTERS.contains(&delimiter) {
                string.to_vec()
            } else {
                let mut out = Vec::with_capacity(string.len());
                let mut i = 0;
                while i < string.len() {
                    if string[i] == b'\\' && string.get(i + 1) == Some(&delimiter) {
                        out.push(delimiter);
                        i += 2;
                    } else {
                        out.push(string[i]);
                        i += 1;
                    }
                }
                out
            }
        } else if interpolation(quote) {
            let mut result = Vec::with_capacity(string.len());
            let mut pos = 0;
            while let Some(offset) = string[pos..].iter().position(|&b| b == b'\\') {
                result.extend_from_slice(&string[pos..pos + offset]);
                pos += offset + 1;
                pos = escape_read(&mut result, string, pos, false, false);
            }
            result.extend_from_slice(&string[pos..]);
            result
        } else {
            let other = match delimiter {
                b'[' => Some(b']'),
                b'(' => Some(b')'),
                b'{' => Some(b'}'),
                b'<' => Some(b'>'),
                _ => None,
            };
            let mut out = Vec::with_capacity(string.len());
            let mut i = 0;
            while i < string.len() {
                if string[i] == b'\\'
                    && let Some(&next) = string.get(i + 1)
                    && (next == b'\\' || next == delimiter || Some(next) == other)
                {
                    out.push(next);
                    i += 2;
                } else {
                    out.push(string[i]);
                    i += 1;
                }
            }
            out
        }
    }
}

fn escape_build(value: u32, control: bool, meta: bool) -> u8 {
    let mut value = value;
    if control {
        value &= 0x9f;
    }
    if meta {
        value |= 0x80;
    }
    value as u8
}

fn push_char(result: &mut Vec<u8>, code: u32) {
    if let Some(c) = char::from_u32(code) {
        let mut buf = [0; 4];
        result.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

/// Reads an escape after a backslash at `pos`; returns the new position.
fn escape_read(result: &mut Vec<u8>, s: &[u8], pos: usize, control: bool, meta: bool) -> usize {
    let Some(&c) = s.get(pos) else {
        return pos;
    };
    if c == b'\n' {
        return pos + 1;
    }
    let simple = match c {
        b'a' => Some(0x07),
        b'b' => Some(0x08),
        b'e' => Some(0x1b),
        b'f' => Some(0x0c),
        b'n' => Some(b'\n'),
        b'r' => Some(b'\r'),
        b's' => Some(b' '),
        b't' => Some(b'\t'),
        b'v' => Some(0x0b),
        b'\\' => Some(b'\\'),
        _ => None,
    };
    if let Some(v) = simple {
        result.push(v);
        return pos + 1;
    }
    let octal = s[pos..]
        .iter()
        .take(3)
        .take_while(|b| (b'0'..=b'7').contains(b))
        .count();
    if octal > 0 {
        let v = u32::from_str_radix(std::str::from_utf8(&s[pos..pos + octal]).unwrap(), 8).unwrap();
        result.push(escape_build(v, control, meta));
        return pos + octal;
    }
    if c == b'x' {
        let hex = s[pos + 1..]
            .iter()
            .take(2)
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        if hex > 0 {
            let v =
                u32::from_str_radix(std::str::from_utf8(&s[pos + 1..pos + 1 + hex]).unwrap(), 16)
                    .unwrap();
            result.push(escape_build(v, control, meta));
            return pos + 1 + hex;
        }
    }
    if c == b'u' {
        let rest = &s[pos + 1..];
        if rest.len() >= 4 && rest[..4].iter().all(u8::is_ascii_hexdigit) {
            let v = u32::from_str_radix(std::str::from_utf8(&rest[..4]).unwrap(), 16).unwrap();
            push_char(result, v);
            return pos + 5;
        }
        if rest.starts_with(b"{}") {
            return pos + 3;
        }
        if rest.starts_with(b"{")
            && let Some(close) = rest.iter().position(|&b| b == b'}')
        {
            let inner = std::str::from_utf8(&rest[1..close]).unwrap_or("");
            for unicode in inner.split_whitespace() {
                if let Ok(v) = u32::from_str_radix(unicode, 16) {
                    push_char(result, v);
                }
            }
            return pos + 1 + close + 1;
        }
    }
    let printable = |b: Option<&u8>| b.is_some_and(|b| (0x20..0x7f).contains(b));
    if c == b'c' {
        let mut p = pos + 1;
        if s.get(p) == Some(&b'\\') && printable(s.get(p + 1)) {
            p += 1;
        }
        if printable(s.get(p)) || s.get(pos + 1) == Some(&b'\\') {
            return escape_read_control(result, s, pos + 1, meta);
        }
    }
    if c == b'C' && s.get(pos + 1) == Some(&b'-') {
        let p = pos + 2;
        if printable(s.get(p)) {
            return escape_read_control(result, s, p, meta);
        }
    }
    if c == b'M' && s.get(pos + 1) == Some(&b'-') {
        let mut p = pos + 2;
        let backslash = s.get(p) == Some(&b'\\');
        if backslash {
            p += 1;
        }
        if printable(s.get(p)) {
            return escape_read(result, s, p, control, true).max(p);
        }
    }
    // Something else after an escape.
    if control && c == b'?' {
        result.push(escape_build(0x7f, false, meta));
    } else {
        result.push(escape_build(u32::from(c), control, meta));
    }
    pos + 1
}

fn escape_read_control(result: &mut Vec<u8>, s: &[u8], pos: usize, meta: bool) -> usize {
    if s.get(pos) == Some(&b'\\') {
        escape_read(result, s, pos + 1, true, meta)
    } else {
        escape_read(result, s, pos, true, meta)
    }
}

fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            out.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

fn chomp(s: &[u8]) -> &[u8] {
    s.strip_suffix(b"\r\n")
        .or_else(|| s.strip_suffix(b"\n"))
        .or_else(|| s.strip_suffix(b"\r"))
        .unwrap_or(s)
}

/// `token.value[/(\\{1,})\n/, 1]&.length || 0`
fn backslashes_before_newline(value: &[u8]) -> usize {
    let Some(nl) = value.iter().position(|&b| b == b'\n') else {
        return 0;
    };
    // The first newline preceded by at least one backslash.
    let mut search_from = 0;
    let mut nl = nl;
    loop {
        let count = value[search_from..nl]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        if count > 0 {
            return count;
        }
        search_from = nl + 1;
        match value[search_from..].iter().position(|&b| b == b'\n') {
            Some(next) => nl = search_from + next,
            None => return 0,
        }
    }
}

fn heredoc_identifier(value: &[u8]) -> Vec<u8> {
    // /<<[-~]?["'`]?(?<heredoc_identifier>.*?)["'`]?\z/
    let mut rest = value.strip_prefix(b"<<").unwrap_or(value);
    if let Some(r) = rest.strip_prefix(b"-").or_else(|| rest.strip_prefix(b"~")) {
        rest = r;
    }
    if let Some(r) = rest
        .strip_prefix(b"\"")
        .or_else(|| rest.strip_prefix(b"'"))
        .or_else(|| rest.strip_prefix(b"`"))
    {
        rest = r;
    }
    if let Some(r) = rest
        .strip_suffix(b"\"")
        .or_else(|| rest.strip_suffix(b"'"))
        .or_else(|| rest.strip_suffix(b"`"))
    {
        rest = r;
    }
    rest.to_vec()
}

fn trim_heredoc_whitespace(string: &[u8], common_whitespace: usize) -> Vec<u8> {
    let mut trimmed_whitespace = 0;
    let mut trimmed_characters = 0;
    while let Some(&c) = string.get(trimmed_characters) {
        if !(c == b'\t' || c == b' ') || trimmed_whitespace >= common_whitespace {
            break;
        }
        if c == b'\t' {
            trimmed_whitespace = (trimmed_whitespace / 8 + 1) * 8;
            if trimmed_whitespace > common_whitespace {
                break;
            }
        } else {
            trimmed_whitespace += 1;
        }
        trimmed_characters += 1;
    }
    string[trimmed_characters..].to_vec()
}

fn simplify_string(value: &[u8], quote: &[u8]) -> bool {
    match quote {
        b"'" => !value.contains(&b'\n'),
        b"\"" => lines(value).iter().all(|line| {
            !line.ends_with(b"\n") || {
                let body = &line[..line.len() - 1];
                body.iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
            }
        }),
        _ => false,
    }
}

fn percent_array_unescape(string: &[u8]) -> Vec<u8> {
    // string.gsub(/(\\)+[ \f\n\r\t\v]/) { |m| m.delete_prefix!("\\") if odd; m }
    let mut out = Vec::with_capacity(string.len());
    let mut i = 0;
    while i < string.len() {
        if string[i] == b'\\' {
            let run = string[i..].iter().take_while(|&&b| b == b'\\').count();
            if let Some(&ws) = string.get(i + run)
                && matches!(ws, b' ' | b'\x0c' | b'\n' | b'\r' | b'\t' | b'\x0b')
            {
                let skip = usize::from(run % 2 == 1);
                out.extend_from_slice(&string[i + skip..i + run + 1]);
                i += run + 1;
                continue;
            }
            out.extend_from_slice(&string[i..i + run]);
            i += run;
        } else {
            out.push(string[i]);
            i += 1;
        }
    }
    out
}

fn percent_array_leading_whitespace(string: &[u8]) -> usize {
    if string.starts_with(b"\n") {
        return 1;
    }
    String::from_utf8_lossy(string)
        .chars()
        .take_while(|&c| c != '\n')
        .map(char::len_utf8)
        .sum()
}

fn interpolation(quote: &[u8]) -> bool {
    !quote.ends_with(b"'")
        && !quote.starts_with(b"%q")
        && !quote.starts_with(b"%w")
        && !quote.starts_with(b"%i")
        && !quote.starts_with(b"%s")
}

fn regexp(quote: &[u8]) -> bool {
    quote == b"/" || quote.starts_with(b"%r")
}

fn percent_array(quote: &[u8]) -> bool {
    quote.starts_with(b"%w")
        || quote.starts_with(b"%W")
        || quote.starts_with(b"%i")
        || quote.starts_with(b"%I")
}

/// `Integer(value).to_s`, or `"0"` when it does not parse.
fn parse_integer(value: &str) -> String {
    let cleaned: String = value.chars().filter(|&c| c != '_').collect();
    let (negative, digits) = match cleaned.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    let lower = digits.to_ascii_lowercase();
    let (radix, body) = if let Some(rest) = lower.strip_prefix("0x") {
        (16, rest.to_owned())
    } else if let Some(rest) = lower.strip_prefix("0b") {
        (2, rest.to_owned())
    } else if let Some(rest) = lower.strip_prefix("0o") {
        (8, rest.to_owned())
    } else if let Some(rest) = lower.strip_prefix("0d") {
        (10, rest.to_owned())
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, lower[1..].to_owned())
    } else {
        (10, lower)
    };
    match i128::from_str_radix(&body, radix) {
        Ok(v) => (if negative { -v } else { v }).to_string(),
        Err(_) => "0".to_owned(),
    }
}

/// `Float(value).to_s`, approximately (Ruby's shortest round-trip format).
fn parse_float(value: &str) -> String {
    let cleaned: String = value.chars().filter(|&c| c != '_').collect();
    match cleaned.parse::<f64>() {
        Ok(f) => ruby_float_to_s(f),
        Err(_) => "0.0".to_owned(),
    }
}

pub fn ruby_float_to_s(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let abs = f.abs();
    if abs != 0.0 && !(1e-4..1e16).contains(&abs) {
        // Scientific notation: "1.0e+20".
        let s = format!("{f:e}");
        let (mantissa, exponent) = s.split_once('e').unwrap();
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        let exponent: i32 = exponent.parse().unwrap();
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let s = format!("{f}");
    if s.contains('.') { s } else { format!("{s}.0") }
}
