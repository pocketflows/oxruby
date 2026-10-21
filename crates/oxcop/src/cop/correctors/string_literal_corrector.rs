//! Port of `lib/rubocop/cop/correctors/string_literal_corrector.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::cop::mixin::util::{ruby_inspect, to_string_literal};

/// `StringLiteralCorrector.correct(corrector, node, style)`.
pub fn correct(corrector: &mut Corrector, node: Node<'_>, style: &str) {
    if node.is(NodeType::Dstr) {
        return;
    }
    let Some(str) = node.str_content() else {
        return;
    };
    if style == "single_quotes" {
        corrector.replace(node.source_range(), to_string_literal(str));
    } else {
        corrector.replace(node.source_range(), ruby_inspect(str));
    }
}
