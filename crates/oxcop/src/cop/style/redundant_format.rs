//! Port of `lib/rubocop/cop/style/redundant_format.rb`. Ruby's
//! `format(string, *format_arguments)` on the literal arguments is done by
//! [`ruby_format`], which covers the `s`, `d`/`i`/`u` and `f` conversions
//! the cop can register (any other conversion makes `all_fields_literal?`
//! false) and gives `None` where `format` raises `ArgumentError`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::utils::format_string::{FormatSequence, FormatString};
use crate::cop::{Cop, Investigation};
use crate::regex;

const RESTRICT_ON_SEND: &[&str] = &["format", "sprintf"];

static FORMAT_WITHOUT_ADDITIONAL_ARGS: NodePattern = NodePattern::new(
    "(send {(const {nil? cbase} :Kernel) nil?} {:format :sprintf} ${str dstr const})",
);
static RATIONAL_NUMBER: NodePattern = NodePattern::new(
    "{rational (send int :/ rational) (begin rational) (begin (send int :/ rational))}",
);
static COMPLEX_NUMBER: NodePattern =
    NodePattern::new("{complex (send int :+ complex) (begin complex) (begin (send int :+ complex))}");
static SPLATTED_ARGUMENTS: NodePattern = NodePattern::new(
    "(send _ {:format :sprintf} <{
      splat
      (hash <kwsplat ...>)
    } ...>)",
);

#[derive(Default)]
pub struct RedundantFormat;

impl Cop for RedundantFormat {
    fn name(&self) -> &'static str {
        "Style/RedundantFormat"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(c) = FORMAT_WITHOUT_ADDITIONAL_ARGS.match_node(node) {
            let value = c[0].node().unwrap();
            if string_with_format_sequence(value) {
                return;
            }
            let replacement = escape_control_chars(value.source());
            register(cx, node, replacement);
            return;
        }
        detect_unnecessary_fields(cx, node);
    }
}

fn register<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, replacement: String) {
    let message = format!(
        "Use `{replacement}` directly instead of `{}`.",
        node.method_name().unwrap_or("")
    );
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        corrector.replace(node.source_range(), &replacement);
    });
}

fn string_with_format_sequence(node: Node<'_>) -> bool {
    let string = match node.ty() {
        NodeType::Str => node.str_content().map(str::to_owned),
        NodeType::Dstr => Some(
            node.child_nodes()
                .filter_map(|c| c.str_content())
                .collect::<String>(),
        ),
        _ => None,
    };
    string.is_some_and(|s| !FormatString::new(&s).format_sequences().is_empty())
}

fn detect_unnecessary_fields<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(first) = node.first_argument().filter(|a| a.is(NodeType::Str)) else {
        return;
    };
    if first.is_heredoc() {
        return;
    }
    let Some(string) = first.str_content() else {
        return;
    };
    let arguments: Vec<Node<'a>> = node.arguments().into_iter().skip(1).collect();
    if arguments.is_empty() || SPLATTED_ARGUMENTS.is_match(node) {
        return;
    }
    if !all_fields_literal(string, arguments.clone()) {
        return;
    }
    let Some(values) = arguments
        .iter()
        .map(|a| argument_value(*a))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let Some(formatted) = ruby_format(string, &values) else {
        return;
    };
    let replacement = quote(&formatted, node);
    register(cx, node, replacement);
}

fn all_fields_literal(string: &str, mut arguments: Vec<Node<'_>>) -> bool {
    let format_string = FormatString::new(string);
    let sequences = format_string.format_sequences();
    if sequences.is_empty() {
        return false;
    }
    let mut count = 0;
    for sequence in sequences {
        if sequence.is_percent() {
            continue;
        }
        if unknown_variable_width(sequence, &arguments) {
            continue;
        }
        let hash = arguments.iter().copied().find(|a| a.is(NodeType::Hash));
        let Some(argument) = find_argument(sequence, &mut arguments, hash) else {
            continue;
        };
        if !matching_argument(sequence, argument) {
            continue;
        }
        if (sequence.width.is_some() || sequence.precision.is_some())
            && argument.is(NodeType::Dstr)
        {
            continue;
        }
        count += 1;
    }
    sequences.len() == count
}

fn unknown_variable_width(sequence: &FormatSequence, arguments: &[Node<'_>]) -> bool {
    if !sequence.is_variable_width() {
        return false;
    }
    let number = sequence.variable_width_argument_number().unwrap_or(1);
    match positional_argument(arguments, number) {
        Some(argument) => !numeric(argument),
        None => true,
    }
}

fn find_argument<'a>(
    sequence: &FormatSequence,
    arguments: &mut Vec<Node<'a>>,
    hash: Option<Node<'a>>,
) -> Option<Node<'a>> {
    if let Some(hash) = hash
        && (sequence.is_annotated() || sequence.is_template())
    {
        let name = sequence.name.as_deref()?;
        return hash.each_node().find_map(|n| {
            (n.is(NodeType::Pair)
                && n.child(0)
                    .is_some_and(|k| k.is(NodeType::Sym) && k.sym_value() == Some(name)))
            .then(|| n.child(1))
            .flatten()
        });
    }
    if sequence.is_variable_width() {
        let index = sequence.variable_width_argument_number().unwrap_or(1) - 1;
        if index >= 0 && (index as usize) < arguments.len() {
            arguments.remove(index as usize);
        }
        if arguments.is_empty() {
            return None;
        }
        return Some(arguments.remove(0));
    }
    if let Some(number) = arg_number(sequence) {
        return positional_argument(arguments, number);
    }
    if arguments.is_empty() {
        None
    } else {
        Some(arguments.remove(0))
    }
}

/// `sequence.arg_number`.
fn arg_number(sequence: &FormatSequence) -> Option<i64> {
    sequence.arg_number.as_deref().and_then(|n| n.parse().ok())
}

fn matching_argument(sequence: &FormatSequence, argument: Node<'_>) -> bool {
    if sequence.is_template() {
        return acceptable_literal(argument);
    }
    match sequence.ty.as_deref() {
        Some("s") => acceptable_literal(argument),
        Some("d" | "i" | "u") => integer(argument),
        Some("f") => float(argument),
        _ => false,
    }
}

/// `ACCEPTABLE_LITERAL_TYPES = %i[str dstr sym dsym numeric boolean nil]`.
fn acceptable_literal(node: Node<'_>) -> bool {
    matches!(
        node.ty(),
        NodeType::Str
            | NodeType::Dstr
            | NodeType::Sym
            | NodeType::Dsym
            | NodeType::Nil
            | NodeType::True
            | NodeType::False
    ) || node.is_numeric()
}

fn positional_argument<'a>(arguments: &[Node<'a>], number: i64) -> Option<Node<'a>> {
    let index = number - 1;
    (index >= 0 && (index as usize) < arguments.len()).then(|| arguments[index as usize])
}

fn numeric(argument: Node<'_>) -> bool {
    argument.is_numeric()
        || argument.is(NodeType::Str)
        || RATIONAL_NUMBER.is_match(argument)
        || COMPLEX_NUMBER.is_match(argument)
}

fn integer(argument: Node<'_>) -> bool {
    numeric(argument) && argument_value(argument).is_some_and(|v| v.to_integer().is_some())
}

fn float(argument: Node<'_>) -> bool {
    numeric(argument) && argument_value(argument).is_some_and(|v| v.to_float().is_some())
}

fn quote(string: &str, node: Node<'_>) -> String {
    let str_node = node.first_argument().unwrap();
    let mut start = str_node.range_source(str_node.loc().begin.unwrap()).to_owned();
    let mut end = str_node.range_source(str_node.loc().end.unwrap()).to_owned();
    if node
        .descendants()
        .any(|d| matches!(d.ty(), NodeType::Dstr | NodeType::Dsym))
    {
        if start == "'" {
            start = "\"".to_owned();
            end = "\"".to_owned();
        } else if let Some(rest) = start.strip_prefix("%q") {
            start = format!("%Q{}", rest.chars().next().map(String::from).unwrap_or_default());
        }
    }
    format!("{start}{}{end}", escape_control_chars(string))
}

/// `string.gsub(/\p{Cc}/) { |s| s.dump[1..-2] }`.
fn escape_control_chars(string: &str) -> String {
    let mut out = String::new();
    for c in string.chars() {
        if c.is_control() {
            match c {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\x0c' => out.push_str("\\f"),
                '\x0b' => out.push_str("\\v"),
                '\x08' => out.push_str("\\b"),
                '\x07' => out.push_str("\\a"),
                '\x1b' => out.push_str("\\e"),
                c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02X}", c as u32)),
                c => out.push_str(&format!("\\u{:04X}", c as u32)),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---- argument values ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Num {
    Int(i128),
    Float(f64),
    Rational(i128, i128),
}

impl Num {
    fn to_s(self) -> String {
        match self {
            Num::Int(i) => i.to_string(),
            Num::Float(f) => float_to_s(f),
            Num::Rational(n, d) => format!("{n}/{d}"),
        }
    }

    fn to_f(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
            Num::Rational(n, d) => n as f64 / d as f64,
        }
    }

    fn to_i(self) -> Option<i128> {
        match self {
            Num::Int(i) => Some(i),
            Num::Float(f) if f.is_finite() => Some(f.trunc() as i128),
            Num::Float(_) => None,
            Num::Rational(n, d) => Some(n / d),
        }
    }

    fn is_negative(self) -> bool {
        match self {
            Num::Int(i) => i < 0,
            Num::Float(f) => f.is_sign_negative(),
            Num::Rational(n, _) => n < 0,
        }
    }

    fn abs(self) -> Num {
        match self {
            Num::Int(i) => Num::Int(i.abs()),
            Num::Float(f) => Num::Float(f.abs()),
            Num::Rational(n, d) => Num::Rational(n.abs(), d),
        }
    }
}

/// A literal argument's `argument_value`.
#[derive(Debug, Clone)]
enum Value {
    Str(String),
    Num(Num),
    Complex(Num, Num),
    Nil,
    Hash(Vec<(Option<String>, Value)>),
}

impl Value {
    /// `to_s` as `format('%s')` does it.
    fn to_s(&self) -> Option<String> {
        Some(match self {
            Value::Str(s) => s.clone(),
            Value::Num(n) => n.to_s(),
            Value::Complex(re, im) => {
                let sign = if im.is_negative() { "-" } else { "+" };
                format!("{}{sign}{}i", re.to_s(), im.abs().to_s())
            }
            Value::Nil => String::new(),
            Value::Hash(_) => return None,
        })
    }

    /// `Integer(value, exception: false)`.
    fn to_integer(&self) -> Option<i128> {
        match self {
            Value::Num(n) => n.to_i(),
            Value::Complex(re, Num::Int(0)) => re.to_i(),
            Value::Str(s) => parse_integer(s),
            _ => None,
        }
    }

    /// `Float(value, exception: false)`.
    fn to_float(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(n.to_f()),
            Value::Complex(re, Num::Int(0)) => Some(re.to_f()),
            Value::Str(s) => parse_float(s),
            _ => None,
        }
    }
}

/// `Kernel#Integer(string)`.
fn parse_integer(s: &str) -> Option<i128> {
    let t = s.trim();
    let (negative, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (radix, digits) = if let Some(r) = t.strip_prefix("0x").or(t.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = t.strip_prefix("0b").or(t.strip_prefix("0B")) {
        (2, r)
    } else if let Some(r) = t.strip_prefix("0o").or(t.strip_prefix("0O")) {
        (8, r)
    } else if t.len() > 1 && t.starts_with('0') {
        (8, &t[1..])
    } else {
        (10, t)
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let value = i128::from_str_radix(&digits.replace('_', ""), radix).ok()?;
    Some(if negative { -value } else { value })
}

/// `Kernel#Float(string)`.
fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim();
    if regex!(r"\A[+-]?(?:\d+(?:_\d+)*)(?:\.\d+(?:_\d+)*)?(?:[eE][+-]?\d+)?\z")
        .find(t)
        .is_none()
    {
        return parse_integer(s).map(|i| i as f64);
    }
    t.replace('_', "").parse().ok()
}

fn argument_value(argument: Node<'_>) -> Option<Value> {
    let argument = if argument.is(NodeType::Begin) {
        argument.child(0)?
    } else {
        argument
    };
    if argument.is(NodeType::Nil) {
        return Some(Value::Nil);
    }
    if argument.is(NodeType::Dsym) {
        return Some(Value::Str(argument.child(0)?.source().to_owned()));
    }
    if argument.is(NodeType::Hash) {
        let mut pairs = Vec::new();
        for pair in argument.pairs() {
            let key = pair.child(0)?;
            let key = match key.ty() {
                NodeType::Sym => Some(key.sym_value()?.to_owned()),
                _ => None,
            };
            pairs.push((key, argument_value(pair.child(1)?)?));
        }
        return Some(Value::Hash(pairs));
    }
    if RATIONAL_NUMBER.is_match(argument) {
        return parse_rational(argument.source()).map(Value::Num);
    }
    if COMPLEX_NUMBER.is_match(argument) {
        return parse_complex(argument.source());
    }
    Some(match (argument.ty(), argument.children().first()) {
        (NodeType::Str, Some(c)) => Value::Str(String::from_utf8_lossy(c.str_bytes()?).into_owned()),
        (NodeType::Dstr, _) => Value::Str(dstr_value(argument)),
        (NodeType::Sym, Some(Child::Sym(s))) => Value::Str(s.clone()),
        (NodeType::Int, Some(Child::Int(i))) => Value::Num(Num::Int(i128::from(*i))),
        (NodeType::Int, Some(Child::BigInt(i))) => Value::Num(Num::Int(i.parse().ok()?)),
        (NodeType::Float, Some(Child::Float(f))) => Value::Num(Num::Float(*f)),
        (NodeType::Rational, _) => parse_rational(argument.source()).map(Value::Num)?,
        (NodeType::Complex, _) => parse_complex(argument.source())?,
        _ => Value::Str(argument.source().to_owned()),
    })
}

/// `DstrNode#value`.
fn dstr_value(node: Node<'_>) -> String {
    node.child_nodes()
        .map(|child| match child.ty() {
            NodeType::Str => child.str_content().unwrap_or("").to_owned(),
            NodeType::Dstr => dstr_value(child),
            _ => child.source().to_owned(),
        })
        .collect()
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

fn rational(n: i128, d: i128) -> Num {
    let g = gcd(n, d).max(1);
    let (n, d) = (n / g, d / g);
    if d < 0 {
        Num::Rational(-n, -d)
    } else {
        Num::Rational(n, d)
    }
}

/// `String#to_r` of a rational literal's source (`3/8r`, `5r`, `1.5r`).
fn parse_rational(source: &str) -> Option<Num> {
    let s = source.trim().trim_end_matches('r');
    let (num, den) = match s.split_once('/') {
        Some((n, d)) => (n.trim(), d.trim()),
        None => (s, "1"),
    };
    let den: i128 = den.replace('_', "").parse().ok()?;
    let num = num.replace('_', "");
    let (n, scale) = match num.split_once('.') {
        Some((int, frac)) => (
            format!("{int}{frac}").parse::<i128>().ok()?,
            10i128.checked_pow(frac.len() as u32)?,
        ),
        None => (num.parse::<i128>().ok()?, 1),
    };
    Some(rational(n, den.checked_mul(scale)?))
}

fn parse_num(s: &str) -> Option<Num> {
    if s.ends_with('r') {
        return parse_rational(s);
    }
    if let Ok(i) = s.replace('_', "").parse::<i128>() {
        return Some(Num::Int(i));
    }
    s.replace('_', "").parse::<f64>().ok().map(Num::Float)
}

/// `Complex(source)` for `1i`, `5+0i`, `1.5i`.
fn parse_complex(source: &str) -> Option<Value> {
    let s = source.trim();
    let body = s.strip_suffix('i')?;
    let split = body
        .char_indices()
        .skip(1)
        .filter(|&(i, c)| (c == '+' || c == '-') && !matches!(body.as_bytes()[i - 1], b'e' | b'E'))
        .map(|(i, _)| i)
        .last();
    let (re, im) = match split {
        Some(i) => (parse_num(&body[..i])?, parse_num(&body[i..])?),
        None => (Num::Int(0), parse_num(if body.is_empty() { "1" } else { body })?),
    };
    Some(Value::Complex(re, im))
}

/// `Float#to_s`.
fn float_to_s(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.to_owned();
    }
    let abs = f.abs();
    if (1e-4..1e16).contains(&abs) {
        let s = format!("{f}");
        if s.contains('.') { s } else { format!("{s}.0") }
    } else {
        let s = format!("{f:e}");
        let (mantissa, exponent) = s.split_once('e').unwrap();
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    }
}

// ---- Kernel#format -----------------------------------------------------------

#[derive(PartialEq)]
enum Mode {
    None,
    Unnumbered,
    Numbered,
    Named,
}

struct Args<'v> {
    values: &'v [Value],
    next: usize,
    mode: Mode,
}

impl<'v> Args<'v> {
    fn set_mode(&mut self, mode: Mode) -> Option<()> {
        if self.mode == Mode::None {
            self.mode = mode;
            Some(())
        } else if self.mode == mode {
            Some(())
        } else {
            None
        }
    }

    fn next_value(&mut self) -> Option<&'v Value> {
        self.set_mode(Mode::Unnumbered)?;
        let value = self.values.get(self.next)?;
        self.next += 1;
        Some(value)
    }

    fn nth(&mut self, n: usize) -> Option<&'v Value> {
        self.set_mode(Mode::Numbered)?;
        self.values.get(n.checked_sub(1)?)
    }

    fn named(&mut self, name: &str) -> Option<&'v Value> {
        self.set_mode(Mode::Named)?;
        let [Value::Hash(pairs)] = self.values else {
            return None;
        };
        pairs
            .iter()
            .find(|(k, _)| k.as_deref() == Some(name))
            .map(|(_, v)| v)
    }

    /// A `*` or `*n$` width/precision argument.
    fn star(&mut self, spec: &str) -> Option<i128> {
        let digits = spec.trim_start_matches('*').trim_end_matches('$');
        let value = if digits.is_empty() {
            self.next_value()?
        } else {
            self.nth(digits.parse().ok()?)?
        };
        value.to_integer()
    }
}

/// `format(string, *values)`, or `None` where it raises `ArgumentError`
/// (or where a construct isn't covered here).
fn ruby_format(string: &str, values: &[Value]) -> Option<String> {
    let format_string = FormatString::new(string);
    let mut args = Args {
        values,
        next: 0,
        mode: Mode::None,
    };
    let mut out = String::new();
    let mut pos = 0;
    for sequence in format_string.format_sequences() {
        let literal = &string[pos..sequence.begin_pos];
        if literal.contains('%') {
            return None;
        }
        out.push_str(literal);
        pos = sequence.end_pos;
        if sequence.is_percent() {
            out.push('%');
            continue;
        }
        out.push_str(&format_sequence(sequence, &mut args)?);
    }
    let rest = &string[pos..];
    if rest.contains('%') {
        return None;
    }
    out.push_str(rest);
    Some(out)
}

fn format_sequence(sequence: &FormatSequence, args: &mut Args<'_>) -> Option<String> {
    let mut flags = String::new();
    let mut arg_number: Option<usize> = arg_number(sequence).map(|n| n as usize);
    for c in regex!(r"\d+\$|.").find_iter(&sequence.flags) {
        let token = &sequence.flags[c.0..c.1];
        if let Some(n) = token.strip_suffix('$') {
            arg_number = Some(n.parse().ok()?);
        } else {
            flags.push_str(token);
        }
    }
    let mut left = flags.contains('-');
    let width = match sequence.width.as_deref() {
        None => None,
        Some(w) if w.starts_with('*') => {
            let w = args.star(w)?;
            if w < 0 {
                left = true;
            }
            Some(w.unsigned_abs() as usize)
        }
        Some(w) => Some(w.parse().ok()?),
    };
    let precision = match sequence.precision.as_deref() {
        None => None,
        Some("") => Some(0),
        Some(p) if p.starts_with('*') => Some(args.star(p)?.max(0) as usize),
        Some(p) => Some(p.parse().ok()?),
    };
    let value = if let Some(name) = &sequence.name {
        args.named(name)?
    } else if let Some(n) = arg_number {
        args.nth(n)?
    } else {
        args.next_value()?
    };
    let ty = if sequence.is_template() {
        "s"
    } else {
        sequence.ty.as_deref()?
    };
    let pad = |body: String, zero: bool, sign_len: usize| -> String {
        let len = body.chars().count();
        let Some(width) = width.filter(|w| *w > len) else {
            return body;
        };
        let fill = width - len;
        if left {
            format!("{body}{}", " ".repeat(fill))
        } else if zero {
            let (sign, digits) = body.split_at(sign_len);
            format!("{sign}{}{digits}", "0".repeat(fill))
        } else {
            format!("{}{body}", " ".repeat(fill))
        }
    };
    let sign = |negative: bool| -> &'static str {
        if negative {
            "-"
        } else if flags.contains('+') {
            "+"
        } else if flags.contains(' ') {
            " "
        } else {
            ""
        }
    };
    match ty {
        "s" => {
            let mut s = value.to_s()?;
            if let Some(p) = precision {
                s = s.chars().take(p).collect();
            }
            Some(pad(s, false, 0))
        }
        "d" | "i" | "u" => {
            let n = value.to_integer()?;
            let mut digits = n.unsigned_abs().to_string();
            if let Some(p) = precision {
                if p == 0 && n == 0 {
                    digits.clear();
                } else if digits.len() < p {
                    digits = format!("{}{digits}", "0".repeat(p - digits.len()));
                }
            }
            let sign = sign(n < 0);
            let zero = flags.contains('0') && precision.is_none();
            Some(pad(format!("{sign}{digits}"), zero, sign.len()))
        }
        "f" => {
            let x = value.to_float()?;
            if !x.is_finite() {
                return None;
            }
            let p = precision.unwrap_or(6);
            let mut digits = format!("{:.*}", p, x.abs());
            if flags.contains('#') && p == 0 {
                digits.push('.');
            }
            let sign = sign(x.is_sign_negative());
            Some(pad(format!("{sign}{digits}"), flags.contains('0'), sign.len()))
        }
        _ => None,
    }
}
