//! Port of `lib/rubocop/cop/style/keyword_arguments_merging.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Provide additional arguments directly rather than using `merge`.";

static MERGE_KWARGS: NodePattern = NodePattern::new(
    "(send _ _
      ...
      (hash
        (kwsplat
          $(send $_ :merge $...))
        ...))",
);

#[derive(Default)]
pub struct KeywordArgumentsMerging;

impl Cop for KeywordArgumentsMerging {
    fn name(&self) -> &'static str {
        "Style/KeywordArgumentsMerging"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Kwsplat]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(ancestor) = node.parent().and_then(|p| p.parent()) else {
            return;
        };
        let Some(captures) = MERGE_KWARGS.match_node(ancestor) else {
            return;
        };
        let merge_node = captures[0].node().unwrap();
        let hash_node = captures[1].node().unwrap();
        let other_hash_node = captures[2].nodes();
        // A block-pass argument (e.g. `merge(other, &block)`) has no keyword
        // equivalent, so spreading it would produce invalid Ruby (`**&block`).
        if other_hash_node.iter().any(|n| n.is(NodeType::BlockPass)) {
            return;
        }
        cx.add_offense_with_correction(merge_node.source_range(), MSG, |corrector| {
            autocorrect(corrector, node, hash_node, &other_hash_node);
        });
    }
}

fn autocorrect(
    corrector: &mut Corrector,
    kwsplat_node: Node<'_>,
    hash_node: Node<'_>,
    other_hash_node: &[Node<'_>],
) {
    let replacement: Vec<String> = other_hash_node
        .iter()
        .map(|node| {
            if node.is(NodeType::Hash) {
                if node.has_braces() {
                    let source = node.source();
                    source[1..source.len() - 1].to_owned()
                } else {
                    node.source().to_owned()
                }
            } else {
                format!("**{}", node.source())
            }
        })
        .collect();
    corrector.replace(
        kwsplat_node.source_range(),
        format!("**{}, {}", hash_node.source(), replacement.join(", ")),
    );
}
