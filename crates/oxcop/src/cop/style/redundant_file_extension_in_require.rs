//! Port of `lib/rubocop/cop/style/redundant_file_extension_in_require.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Redundant `.rb` file extension detected.";

#[derive(Default)]
pub struct RedundantFileExtensionInRequire;

impl Cop for RedundantFileExtensionInRequire {
    fn name(&self) -> &'static str {
        "Style/RedundantFileExtensionInRequire"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["require", "require_relative"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // (send nil? {:require :require_relative} $str_type?)
        if node.receiver().is_some() || node.children().len() != 3 {
            return;
        }
        let Some(name_node) = node.first_argument().filter(|a| a.is(NodeType::Str)) else {
            return;
        };
        if !name_node
            .value()
            .and_then(|v| v.str_bytes())
            .is_some_and(|v| v.ends_with(b".rb"))
        {
            return;
        }
        let end_of_path_string = name_node.source_range().end_pos;
        let extension_range = range_between(end_of_path_string - 4, end_of_path_string - 1);
        let offset = extension_range.begin_pos - name_node.source_range().begin_pos;
        if name_node.source().as_bytes()[..offset].ends_with(b"\\") {
            return;
        }
        cx.add_offense_with_correction(extension_range, MSG, |corrector| {
            corrector.remove(extension_range);
        });
    }
}
