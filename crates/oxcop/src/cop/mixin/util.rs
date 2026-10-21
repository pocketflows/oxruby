//! Port of `lib/rubocop/cop/util.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

/// `comment_line?(line_source)`.
pub fn comment_line(line_source: &str) -> bool {
    regex!(r"^\s*#").find(line_source).is_some()
}

/// `parentheses?(node)`: `loc.end` is `)`.
pub fn parentheses(node: Node<'_>) -> bool {
    node.loc_is(node.loc().end, ")")
}

/// `args_begin(node)`.
pub fn args_begin(node: Node<'_>) -> Range {
    let loc = node.loc();
    let selector = if matches!(node.ty(), NodeType::Super | NodeType::Yield) {
        loc.keyword
    } else if node.is_any_def() {
        loc.name
    } else {
        loc.selector
    }
    .unwrap();
    Range::new(selector.end_pos, selector.end_pos + 1)
}

/// `args_end(node)`.
pub fn args_end(node: Node<'_>) -> Range {
    node.source_range().end()
}

/// `add_parentheses(node, corrector)`.
pub fn add_parentheses(src: &str, node: Node<'_>, corrector: &mut Corrector) {
    if node.is(NodeType::Args) {
        let arguments_range = node.source_range();
        let args_with_space =
            range_with_surrounding_space(src, arguments_range, SurroundingSpace::side(Side::Left));
        let leading_space = Range::new(args_with_space.begin_pos, arguments_range.begin_pos);
        corrector.replace(leading_space, "(");
        corrector.insert_after(arguments_range, ")");
    } else if !responds_to_arguments(node) {
        corrector.wrap(node.source_range(), "(", ")");
    } else if node.arguments().is_empty() {
        corrector.insert_after(node.source_range(), "()");
    } else {
        let args_begin = args_begin(node);
        corrector.remove(args_begin);
        corrector.insert_before(args_begin, "(");
        corrector.insert_after(args_end(node), ")");
    }
}

/// Node classes with an `arguments` method.
pub fn responds_to_arguments(node: Node<'_>) -> bool {
    use NodeType::*;
    matches!(
        node.ty(),
        Send | Csend
            | Super
            | Zsuper
            | Yield
            | Defined
            | Def
            | Defs
            | Block
            | Numblock
            | Itblock
            | Lambda
            | Index
            | Indexasgn
            | Return
            | Break
            | Next
    )
}

/// `begins_its_line?(range)`.
pub fn begins_its_line(ps: &ProcessedSource<'_>, range: Range) -> bool {
    let line = ps.source_line(ps.line_of(range.begin_pos));
    let column = ps.column_of(range.begin_pos);
    line.chars()
        .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
        == Some(column)
}

/// `first_part_of_call_chain(node)`.
pub fn first_part_of_call_chain(node: Node<'_>) -> Option<Node<'_>> {
    let mut node = Some(node);
    while let Some(n) = node {
        if n.is_call() {
            node = n.receiver();
        } else if n.is_any_block() {
            node = n.send_node();
        } else {
            break;
        }
    }
    node
}

/// `double_quotes_required?(string)`.
pub fn double_quotes_required(string: &str) -> bool {
    // The Ruby regexp is in extended mode (`/x`).
    static RE: std::sync::LazyLock<onig::Regex> = std::sync::LazyLock::new(|| {
        onig::Regex::with_options(
            r#"'|(?<! \\) \\{2}* \\ (?![\\"])"#,
            onig::RegexOptions::REGEX_OPTION_EXTEND,
            onig::Syntax::ruby(),
        )
        .unwrap()
    });
    RE.find(string).is_some()
}

/// `needs_escaping?(string)`.
pub fn needs_escaping(string: &str) -> bool {
    double_quotes_required(&escape_string(string))
}

/// `escape_string(string)`: `string.inspect[1..-2]` with `\"` unescaped.
pub fn escape_string(string: &str) -> String {
    let inspected = ruby_inspect(string);
    inspected[1..inspected.len() - 1].replace("\\\"", "\"")
}

/// `to_string_literal(string)`.
pub fn to_string_literal(string: &str) -> String {
    if needs_escaping(string) {
        ruby_inspect(string)
    } else {
        format!("'{}'", string.replace('\\', "\\\\").replace("\\\"", "\""))
    }
}

/// Ruby's `String#inspect` for a UTF-8 string.
pub fn ruby_inspect(string: &str) -> String {
    let mut out = String::with_capacity(string.len() + 2);
    out.push('"');
    let chars: Vec<char> = string.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            '\x08' => out.push_str("\\b"),
            '\x07' => out.push_str("\\a"),
            '\x1b' => out.push_str("\\e"),
            '#' if matches!(chars.get(i + 1), Some('{' | '$' | '@')) => out.push_str("\\#"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                // UTF-8 strings escape code points as `\uXXXX`.
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if is_printable(c) => out.push(c),
            c if (c as u32) <= 0xffff => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push_str(&format!("\\u{{{:X}}}", c as u32)),
        }
    }
    out.push('"');
    out
}

fn is_printable(c: char) -> bool {
    !(c.is_control()
        || matches!(c as u32, 0x2028 | 0x2029 | 0xfeff)
        || (0xd800..0xe000).contains(&(c as u32)))
}

/// `trim_string_interpolation_escape_character(str)`.
pub fn trim_string_interpolation_escape_character(string: &str) -> String {
    regex!(r"\\\#\{(.*?)\}").replace_all(string, |caps: &onig::Captures<'_>| {
        format!("#{{{}}}", caps.at(1).unwrap_or(""))
    })
}

/// `same_line?(a, b)` for ranges.
pub fn same_line(ps: &ProcessedSource<'_>, a: Range, b: Range) -> bool {
    ps.line_of(a.begin_pos) == ps.line_of(b.begin_pos)
}

/// `indent(node, offset:)`.
pub fn indent(node: Node<'_>, offset: isize) -> String {
    " ".repeat((node.column() as isize + offset).max(0) as usize)
}

/// `to_supported_styles(enforced_style)`.
pub fn to_supported_styles(enforced_style: &str) -> String {
    let s = enforced_style.strip_prefix("Enforced").map_or_else(
        || enforced_style.to_owned(),
        |rest| format!("Supported{rest}"),
    );
    s.replacen("Style", "Styles", 1)
}

/// `interpret_string_escapes(string)`: port of `StringInterpreter`.
pub fn interpret_string_escapes(string: &str) -> String {
    let b: Vec<char> = string.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != '\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let c = b[i + 1];
        i += 2;
        match c {
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'e' => out.push('\x1b'),
            'f' => out.push('\x0c'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            's' => out.push(' '),
            't' => out.push('\t'),
            'v' => out.push('\x0b'),
            '\n' => {}
            'u' => {
                if i < b.len() && b[i] == '{' {
                    let close = b[i..].iter().position(|&c| c == '}').map(|p| i + p);
                    if let Some(close) = close {
                        let inner: String = b[i + 1..close].iter().collect();
                        for hex in inner.split_whitespace() {
                            if let Some(ch) =
                                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                            {
                                out.push(ch);
                            }
                        }
                        i = close + 1;
                    }
                } else {
                    let hex: String = b[i..(i + 4).min(b.len())].iter().collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                        i += 4;
                    }
                }
            }
            'x' => {
                let hex: String = b[i..]
                    .iter()
                    .take(2)
                    .take_while(|c| c.is_ascii_hexdigit())
                    .collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                    i += hex.len();
                }
            }
            '0'..='7' => {
                let oct: String = b[i - 1..]
                    .iter()
                    .take(3)
                    .take_while(|c| ('0'..='7').contains(c))
                    .collect();
                if let Some(ch) = u32::from_str_radix(&oct, 8).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
                i += oct.len() - 1;
            }
            other => out.push(other),
        }
    }
    out
}

/// Ruby's `Symbol#inspect` (`:foo`, `:foo?`, `:+`, `:"foo bar"`).
pub fn symbol_inspect(name: &str) -> String {
    if symbol_name_is_simple(name) {
        format!(":{name}")
    } else {
        format!(":{}", ruby_inspect(name))
    }
}

/// `rb_enc_symname_p`: whether a symbol prints without quotes.
fn symbol_name_is_simple(name: &str) -> bool {
    const OPERATORS: &[&str] = &[
        "[]", "[]=", "+", "-", "*", "/", "%", "**", "==", "===", "!=", "=~", "!~", "!", "<=>", "<",
        "<=", ">", ">=", "<<", ">>", "&", "|", "^", "~", "+@", "-@", "`",
    ];
    if OPERATORS.contains(&name) {
        return true;
    }
    let is_ident_start = |c: char| c == '_' || c.is_alphabetic() || !c.is_ascii();
    let is_ident_char = |c: char| c == '_' || c.is_alphanumeric() || !c.is_ascii();
    let ident = |s: &str| {
        let mut chars = s.chars();
        chars.next().is_some_and(is_ident_start) && chars.all(is_ident_char)
    };
    if let Some(rest) = name.strip_prefix('$') {
        if rest.len() == 1 && r#"~*$?!@/\;,.=:<>"&`'+0"#.contains(rest) {
            return true;
        }
        if let Some(opt) = rest.strip_prefix('-') {
            let mut chars = opt.chars();
            return chars.next().is_some_and(is_ident_char) && chars.next().is_none();
        }
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        return ident(rest);
    }
    if let Some(rest) = name.strip_prefix("@@") {
        return ident(rest);
    }
    if let Some(rest) = name.strip_prefix('@') {
        return ident(rest);
    }
    let base = name
        .strip_suffix(['?', '!', '='])
        .filter(|b| !b.ends_with(['?', '!', '=']))
        .unwrap_or(name);
    ident(base)
}

/// `parse_regexp(text)`: `Regexp::Parser.parse(text)`, `None` for an
/// invalid regexp.
pub fn parse_regexp(text: &str) -> Option<crate::regexp_parser::Tree> {
    crate::regexp_parser::parse(text, false, false).ok()
}
