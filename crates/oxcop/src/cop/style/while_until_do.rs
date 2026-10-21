//! Port of `lib/rubocop/cop/style/while_until_do.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct WhileUntilDo;

impl Cop for WhileUntilDo {
    fn name(&self) -> &'static str {
        "Style/WhileUntilDo"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_multiline() && node.has_do()) {
            return;
        }
        let begin = node.loc().begin.unwrap();
        if node
            .body()
            .is_some_and(|body| same_line(cx.processed_source, begin, body.source_range()))
        {
            return;
        }
        let message = format!("Do not use `do` with multi-line `{}`.", node.keyword());
        cx.add_offense_with_correction(begin, &message, |corrector| {
            let do_range = node.condition().unwrap().source_range().end().join(begin);
            corrector.remove(do_range);
        });
    }
}
