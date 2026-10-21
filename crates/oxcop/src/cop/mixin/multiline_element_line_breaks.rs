//! Port of `lib/rubocop/cop/mixin/multiline_element_line_breaks.rb`.

use crate::ast::Node;
use crate::cop::Investigation;
use crate::cop::correctors::empty_line_corrector;
use crate::cop::mixin::util::same_line;

/// `check_line_breaks(node, children, ignore_last:)`, registering `message`.
pub fn check_line_breaks(
    cx: &mut Investigation<'_, '_>,
    children: &[Node<'_>],
    ignore_last: bool,
    message: &str,
) {
    if all_on_same_line(cx, children, ignore_last) {
        return;
    }
    let mut last_seen_line: isize = -1;
    for child in children {
        if last_seen_line >= child.first_line() as isize {
            let range = child.source_range();
            cx.add_offense_with_correction(range, message, |corrector| {
                empty_line_corrector::insert_before(corrector, range);
            });
        } else {
            last_seen_line = child.last_line() as isize;
        }
    }
}

fn all_on_same_line(cx: &Investigation<'_, '_>, nodes: &[Node<'_>], ignore_last: bool) -> bool {
    let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
        return true;
    };
    if ignore_last {
        return same_line(
            cx.processed_source,
            first.source_range(),
            last.source_range(),
        );
    }
    first.first_line() == last.last_line()
}
