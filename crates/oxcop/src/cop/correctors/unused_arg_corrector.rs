//! Port of `lib/rubocop/cop/correctors/unused_arg_corrector.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_with_surrounding_comma, range_with_surrounding_space,
};
use crate::processed_source::ProcessedSource;

/// Autocorrects an unused argument.
pub fn correct(corrector: &mut Corrector, processed_source: &ProcessedSource<'_>, node: Node<'_>) {
    if matches!(node.ty(), NodeType::Kwarg | NodeType::Kwoptarg) {
        return;
    }

    if node.is(NodeType::Blockarg) {
        correct_for_blockarg_type(corrector, processed_source, node);
    } else {
        let variable_name = if node.is(NodeType::Optarg) {
            node.child_sym(0).unwrap_or_default().to_owned()
        } else {
            // Extract only a var name without splat (`*`)
            node.source().trim_start_matches('*').to_owned()
        };

        if let Some(name) = node.loc().name {
            corrector.replace(name, format!("_{variable_name}"));
        }
    }
}

fn correct_for_blockarg_type(
    corrector: &mut Corrector,
    processed_source: &ProcessedSource<'_>,
    node: Node<'_>,
) {
    let src = processed_source.buffer_source();
    let range =
        range_with_surrounding_space(src, node.source_range(), SurroundingSpace::side(Side::Left));
    let range = range_with_surrounding_comma(src, range, Side::Left);

    corrector.remove(range);
}
