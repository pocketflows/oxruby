//! Port of `lib/rubocop/cop/style/percent_literal_delimiters.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::percent_literal::{begin_source, percent_type, process};
use crate::cop::mixin::preferred_delimiters::delimiters;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct PercentLiteralDelimiters;

impl Cop for PercentLiteralDelimiters {
    fn name(&self) -> &'static str {
        "Style/PercentLiteralDelimiters"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Array,
            NodeType::Regexp,
            NodeType::Str,
            NodeType::Dstr,
            NodeType::Sym,
            NodeType::Xstr,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let types: &[&str] = match node.ty() {
            NodeType::Array => &["%w", "%W", "%i", "%I"],
            NodeType::Regexp => &["%r"],
            NodeType::Str | NodeType::Dstr => &["%", "%Q", "%q"],
            NodeType::Sym => &["%s"],
            _ => &["%x"],
        };
        if !process(node, types) {
            return;
        }
        let ty = percent_type(node);
        let Some((opening, closing)) = delimiters(cx.config, ty) else {
            return;
        };
        let used = begin_source(node).unwrap_or("").chars().last();
        if opening.chars().next() == used
            || contains_delimiter(node, &[opening.as_str(), closing.as_str()])
            || include_same_character_as_used_for_delimiter(node, ty, used)
        {
            return;
        }
        let message = format!("`{ty}`-literals should be delimited by `{opening}` and `{closing}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.loc().begin.unwrap(), format!("{ty}{opening}"));
            corrector.replace(node.loc().end.unwrap(), &closing);
        });
    }
}

fn include_same_character_as_used_for_delimiter(node: Node<'_>, ty: &str, used: Option<char>) -> bool {
    if !matches!(ty, "%w" | "%i") {
        return false;
    }
    let Some(used) = used else {
        return false;
    };
    let pair: Vec<String> = match used {
        '(' => vec!["(".into(), ")".into()],
        '[' => vec!["[".into(), "]".into()],
        '{' => vec!["{".into(), "}".into()],
        '<' => vec!["<".into(), ">".into()],
        c => vec![c.to_string()],
    };
    let pair: Vec<&str> = pair.iter().map(String::as_str).collect();
    contains_delimiter(node, &pair)
}

/// `node.children.filter_map { string_source }.any?(Regexp.union(delimiters))`.
fn contains_delimiter(node: Node<'_>, delimiters: &[&str]) -> bool {
    let delimiters: Vec<&str> = delimiters.iter().copied().filter(|d| !d.is_empty()).collect();
    let matches = |s: &str| delimiters.iter().any(|d| s.contains(d));
    node.children().iter().any(|child| match child {
        Child::Str(s) => matches(s),
        Child::Bytes(b) => matches(&String::from_utf8_lossy(b)),
        Child::Sym(s) => matches(s),
        Child::Node(id) => {
            let n = node.ast().node(*id);
            matches!(n.ty(), NodeType::Str | NodeType::Sym) && matches(n.source())
        }
        _ => false,
    })
}
