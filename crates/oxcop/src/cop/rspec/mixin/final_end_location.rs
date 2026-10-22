//! Port of rubocop-rspec's `lib/rubocop/cop/rspec/mixin/final_end_location.rb`.
//!
//! Helps find the true end location of nodes which might contain heredocs.

use crate::ast::{Node, NodeType};
use crate::processed_source::Range;

/// `final_end_location(start_node)`: `loc.end`, or the end of the last
/// heredoc in the node when that is on a later line. `None` when the node
/// has no `end` (upstream would fail on `nil.line`).
pub fn final_end_location(start_node: Node<'_>) -> Option<Range> {
    let heredoc_endings = start_node
        .each_node()
        .filter(|node| {
            matches!(node.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr)
                && node.is_heredoc()
        })
        .filter_map(|node| node.loc().heredoc_end);

    let mut max = start_node.loc().end?;
    let mut max_line = start_node.line_of(max);
    // `max_by(&:line)`: the first of the latest lines.
    for range in heredoc_endings {
        let line = start_node.line_of(range);
        if line > max_line {
            max = range;
            max_line = line;
        }
    }
    Some(max)
}
