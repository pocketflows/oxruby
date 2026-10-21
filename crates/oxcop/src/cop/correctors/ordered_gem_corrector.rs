//! Port of `lib/rubocop/cop/correctors/ordered_gem_corrector.rb`: this
//! autocorrects gem dependency order.

use crate::ast::Node;
use crate::cop::Corrector;
use crate::cop::mixin::ordered_gem_node::get_source_range;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::processed_source::{ProcessedSource, Range};

pub fn correct(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    previous_declaration: Node<'_>,
    comments_as_separators: bool,
    corrector: &mut Corrector,
) {
    let current_range = declaration_with_comment(ps, node, comments_as_separators);
    let previous_range = declaration_with_comment(ps, previous_declaration, comments_as_separators);
    let source = ps.buffer_source();
    let current_source = &source[current_range.begin_pos..current_range.end_pos];
    if current_source.ends_with('\n') {
        corrector.swap(source, current_range, previous_range);
    } else {
        corrector.insert_before(previous_range, format!("{current_source}\n"));
        corrector.remove(current_range);
    }
}

fn declaration_with_comment(ps: &ProcessedSource<'_>, node: Node<'_>, separators: bool) -> Range {
    let begin_pos =
        range_by_whole_lines(ps, get_source_range(ps, node, separators), false).begin_pos;
    let end_line = ps.line_of(node.source_range().end_pos);
    let end_pos = range_by_whole_lines(ps, ps.line_range(end_line), true).end_pos;
    Range::new(begin_pos, end_pos)
}
