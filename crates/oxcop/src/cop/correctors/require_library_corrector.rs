//! Port of `lib/rubocop/cop/correctors/require_library_corrector.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;

/// `RequireLibraryCorrector.correct(corrector, node, library_name)`.
pub fn correct(corrector: &mut Corrector, node: Node<'_>, library_name: &str) {
    let mut node = node;
    while let Some(parent) = node.parent() {
        node = parent;
    }
    if node.is(NodeType::Begin)
        && let Some(first) = node.child(0)
    {
        node = first;
    }
    corrector.insert_before(node.source_range(), require_statement(library_name));
}

pub fn require_statement(library_name: &str) -> String {
    format!("require '{library_name}'\n")
}
