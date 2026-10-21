//! Port of `lib/rubocop/cop/style/symbol_array.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::array_min_size::ArrayStyleState;
use crate::cop::mixin::array_syntax::bracketed_array_of;
use crate::cop::mixin::percent_array::{
    build_bracketed_array_with_appropriate_whitespace, check_bracketed_array,
    check_percent_array, PercentArray,
};
use crate::cop::mixin::util::{to_string_literal, trim_string_interpolation_escape_character};
use crate::cop::{Cop, Investigation};
use crate::regex;

const DELIMITERS: &[&str] = &["[", "]", "(", ")"];
const SPECIAL_GVARS: &[&str] = &[
    "$!", "$\"", "$$", "$&", "$'", "$*", "$+", "$,", "$/", "$;", "$:", "$.", "$<", "$=", "$>",
    "$?", "$@", "$\\", "$_", "$`", "$~", "$0", "$-0", "$-F", "$-I", "$-K", "$-W", "$-a", "$-d",
    "$-i", "$-l", "$-p", "$-v", "$-w",
];
const REDEFINABLE_OPERATORS: &[&str] = &[
    "|", "^", "&", "<=>", "==", "===", "=~", ">", ">=", "<", "<=", "<<", ">>", "+", "-", "*", "/",
    "%", "**", "~", "+@", "-@", "[]", "[]=", "`", "!", "!=", "!~",
];

#[derive(Default)]
pub struct SymbolArray {
    array_style: ArrayStyleState,
}

impl Cop for SymbolArray {
    fn name(&self) -> &'static str {
        "Style/SymbolArray"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if bracketed_array_of(NodeType::Sym, node) {
            if complex_content(node) {
                return;
            }
            check_bracketed_array(self, cx, node, "i");
        } else if node.is_percent_literal_of("symbol") {
            check_percent_array(self, cx, node);
        }
    }
}

impl PercentArray for SymbolArray {
    const ARRAY_MSG: &'static str = "Use %<prefer>s for an array of symbols.";
    fn array_style_state(&mut self) -> &mut ArrayStyleState {
        &mut self.array_style
    }

    const PERCENT_MSG: &'static str = "Use `%i` or `%I` for an array of symbols.";

    fn invalid_percent_array_contents(&self, _cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
        complex_content(node)
    }

    fn build_bracketed_array(&self, _cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
        if node.child_nodes().next().is_none() {
            return "[]".to_owned();
        }
        let syms: Vec<String> = node
            .child_nodes()
            .map(|c| {
                if c.is(NodeType::Dsym) {
                    let string_literal = to_string_literal(c.source());
                    format!(":{}", trim_string_interpolation_escape_character(&string_literal))
                } else {
                    to_symbol_literal(c.sym_value().unwrap_or_default())
                }
            })
            .collect();
        build_bracketed_array_with_appropriate_whitespace(&syms, node)
    }
}

fn complex_content(node: Node<'_>) -> bool {
    for sym in node.child_nodes() {
        if DELIMITERS.contains(&sym.source()) {
            return false;
        }
        let content: String = sym
            .children()
            .iter()
            .map(|c| match c {
                Child::Node(id) => sym.ast().node(*id).source().to_owned(),
                Child::Sym(s) | Child::Str(s) => s.clone(),
                _ => String::new(),
            })
            .collect();
        let without_pairs = regex!(r"(\[[^\s\[\]]*\])|(\([^\s()]*\))")
            .replace_all(&content, |_: &onig::Captures<'_>| String::new());
        if content.contains(' ') || DELIMITERS.iter().any(|d| without_pairs.contains(d)) {
            return true;
        }
    }
    false
}

fn to_symbol_literal(string: &str) -> String {
    if symbol_without_quote(string) {
        format!(":{string}")
    } else {
        format!(":{}", to_string_literal(string))
    }
}

fn symbol_without_quote(string: &str) -> bool {
    regex!(r"\A[a-zA-Z_][a-zA-Z0-9_]*[!?]?\z").find(string).is_some()
        || regex!(r"\A@@?[a-zA-Z_][a-zA-Z0-9_]*\z").find(string).is_some()
        || regex!(r"\A\$[1-9][0-9]*\z").find(string).is_some()
        || regex!(r"\A\$[a-zA-Z_][a-zA-Z0-9_]*\z").find(string).is_some()
        || SPECIAL_GVARS.contains(&string)
        || REDEFINABLE_OPERATORS.contains(&string)
}
