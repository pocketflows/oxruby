//! Port of `lib/rubocop/cop/style/redundant_argument.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::{SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const NO_RECEIVER_METHODS: &[&str] = &["exit", "exit!"];

#[derive(Default)]
pub struct RedundantArgument;

impl Cop for RedundantArgument {
    fn name(&self) -> &'static str {
        "Style/RedundantArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method_name = node.method_name().unwrap_or("");
        if !NO_RECEIVER_METHODS.contains(&method_name) && node.receiver().is_none() {
            return;
        }
        if node.arguments().len() != 1 || !redundant_argument(cx, node, method_name) {
            return;
        }
        let offense_range = argument_range(cx, node);
        let message = format!(
            "Argument {} is redundant because it is implied by default.",
            node.first_argument().unwrap().source()
        );
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            corrector.remove(offense_range);
        });
    }
}

/// A target argument: a node's `value`, or the node itself.
enum Target<'a> {
    Bytes(Vec<u8>),
    Sym(&'a str),
    Int(String),
    Float(f64),
    Node(Node<'a>),
}

fn redundant_argument(cx: &Investigation<'_, '_>, node: Node<'_>, method_name: &str) -> bool {
    let Some(redundant) = redundant_arg_for_method(cx, method_name) else {
        return false;
    };
    let arg = node.first_argument().unwrap();
    let target = match (arg.ty(), arg.children().first()) {
        (NodeType::Str, Some(c)) => Target::Bytes(c.str_bytes().unwrap_or_default().to_vec()),
        (NodeType::Dstr, _) => Target::Bytes(dstr_value(arg)),
        (NodeType::Sym, Some(Child::Sym(s))) => Target::Sym(s),
        (NodeType::Int, Some(Child::Int(i))) => Target::Int(i.to_string()),
        (NodeType::Int, Some(Child::BigInt(i))) => Target::Int(i.clone()),
        (NodeType::Float, Some(Child::Float(f))) => Target::Float(*f),
        _ => Target::Node(arg),
    };
    argument_matched(&target, &redundant)
}

/// `DstrNode#value`.
fn dstr_value(node: Node<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    for child in node.child_nodes() {
        match (child.ty(), child.children().first()) {
            (NodeType::Str, Some(c)) => out.extend_from_slice(c.str_bytes().unwrap_or_default()),
            (NodeType::Dstr, _) => out.extend(dstr_value(child)),
            _ => out.extend_from_slice(child.source().as_bytes()),
        }
    }
    out
}

/// `cop_config['Methods'].fetch(method_name).inspect`.
fn redundant_arg_for_method(cx: &Investigation<'_, '_>, method_name: &str) -> Option<String> {
    let methods = cx.cop_config_value("Methods")?.as_mapping()?;
    let arg = methods.get(method_name)?;
    Some(inspect_value(arg))
}

fn inspect_value(value: &Value) -> String {
    match value {
        Value::Null => "nil".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => float_inspect(f),
            _ => n.to_string(),
        },
        Value::String(s) => ruby_inspect(s),
        Value::Tagged(tagged) if tagged.tag == "binary" => match &tagged.value {
            Value::String(s) => bytes_inspect(&base64_decode(s)),
            other => inspect_value(other),
        },
        Value::Tagged(tagged) => inspect_value(&tagged.value),
        _ => String::new(),
    }
}

fn argument_matched(target: &Target<'_>, redundant: &str) -> bool {
    let argument = match target {
        Target::Node(node) => node.source().to_owned(),
        Target::Bytes(bytes) => {
            if exclude_cntrl_character(bytes, redundant) {
                bytes_inspect(bytes)
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
        Target::Sym(s) => {
            if exclude_cntrl_character(s.as_bytes(), redundant) {
                symbol_inspect(s)
            } else {
                (*s).to_owned()
            }
        }
        Target::Int(i) => i.clone(),
        Target::Float(f) => float_inspect(*f),
    };
    argument == redundant
}

fn exclude_cntrl_character(target: &[u8], redundant: &str) -> bool {
    let Ok(target) = std::str::from_utf8(target) else {
        return true;
    };
    let cntrl = regex!(r"[[:cntrl:]]");
    let quoted = regex!(r"\A'").replace(target, "\"");
    let quoted = regex!(r"'\z").replace(&quoted, "\"");
    cntrl.find(&quoted).is_none() || cntrl.find(redundant).is_none()
}

/// `String#inspect`, with `\xHH` for bytes that aren't valid UTF-8.
fn bytes_inspect(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return ruby_inspect(s);
    }
    let mut out = String::from("\"");
    for chunk in bytes.utf8_chunks() {
        let valid = ruby_inspect(chunk.valid());
        out.push_str(&valid[1..valid.len() - 1]);
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02X}"));
        }
    }
    out.push('"');
    out
}

fn symbol_inspect(s: &str) -> String {
    if regex!(r"\A(?:[A-Za-z_][A-Za-z0-9_]*[?!=]?|\[\]=?|[+\-*/%<>!~^&|]|\*\*|<=>|==|===|=~|!=|!~|<<|>>|<=|>=|[+-]@)\z")
        .find(s)
        .is_some()
    {
        format!(":{s}")
    } else {
        format!(":{}", ruby_inspect(s))
    }
}

/// `Float#inspect` for ordinary values.
fn float_inspect(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        format!("{f:?}")
    }
}

fn base64_decode(input: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    out
}

fn argument_range(cx: &Investigation<'_, '_>, node: Node<'_>) -> Range {
    if node.is_parenthesized() {
        range_between(
            node.loc().begin.unwrap().begin_pos,
            node.loc().end.unwrap().end_pos,
        )
    } else {
        range_with_surrounding_space(
            cx.processed_source.buffer_source(),
            node.first_argument().unwrap().source_range(),
            SurroundingSpace {
                newlines: false,
                ..SurroundingSpace::default()
            },
        )
    }
}
