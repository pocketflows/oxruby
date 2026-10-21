//! Port of `lib/rubocop/cop/lint/uri_escape_unescape.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const ALTERNATE_METHODS_OF_URI_ESCAPE: &[&str] = &[
    "CGI.escape",
    "URI.encode_www_form",
    "URI.encode_www_form_component",
];
const ALTERNATE_METHODS_OF_URI_UNESCAPE: &[&str] = &[
    "CGI.unescape",
    "URI.decode_www_form",
    "URI.decode_www_form_component",
];

static URI_ESCAPE_UNESCAPE: NodePattern = NodePattern::new(
    "(send
       (const ${nil? cbase} :URI) ${:escape :encode :unescape :decode}
       ...)",
);

#[derive(Default)]
pub struct UriEscapeUnescape;

impl Cop for UriEscapeUnescape {
    fn name(&self) -> &'static str {
        "Lint/UriEscapeUnescape"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["escape", "encode", "unescape", "decode"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = URI_ESCAPE_UNESCAPE.match_node(node) else {
            return;
        };
        let top_level = !captures[0].is_nil();
        let Some(obsolete_method) = captures[1].sym() else {
            return;
        };

        let replacements = if matches!(obsolete_method, "escape" | "encode") {
            ALTERNATE_METHODS_OF_URI_ESCAPE
        } else {
            ALTERNATE_METHODS_OF_URI_UNESCAPE
        };
        let double_colon = if top_level { "::" } else { "" };
        let message = format!(
            "`{double_colon}URI.{obsolete_method}` method is obsolete and should not be used. Instead, use `{}`, `{}` or `{}` depending on your specific use case.",
            replacements[0], replacements[1], replacements[2]
        );
        cx.add_offense(node.source_range(), &message);
    }
}
