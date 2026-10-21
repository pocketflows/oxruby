//! Port of `lib/rubocop/cop/style/string_hash_keys.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::util::symbol_inspect;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer symbols instead of strings as hash keys.";

static STRING_HASH_KEY: NodePattern = NodePattern::new("(pair (str _) _)");

static RECEIVE_ENVIRONMENTS_METHOD: NodePattern = NodePattern::new(
    "{
       ^^(send (const {nil? cbase} :IO) :popen ...)
       ^^(send (const {nil? cbase} :Open3)
           {:capture2 :capture2e :capture3 :popen2 :popen2e :popen3} ...)
       ^^^(send (const {nil? cbase} :Open3)
           {:pipeline :pipeline_r :pipeline_rw :pipeline_start :pipeline_w} ...)
       ^^(send {nil? (const {nil? cbase} :Kernel)} {:spawn :system} ...)
       ^^(send _ {:gsub :gsub!} ...)
     }",
);

#[derive(Default)]
pub struct StringHashKeys;

impl Cop for StringHashKeys {
    fn name(&self) -> &'static str {
        "Style/StringHashKeys"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Pair]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !STRING_HASH_KEY.is_match(node) {
            return;
        }
        let key = node.key().unwrap();
        if key.is_heredoc() {
            return;
        }
        // Invalid encodings are kept as bytes.
        let Some(Child::Str(key_content)) = key.value() else {
            return;
        };
        if RECEIVE_ENVIRONMENTS_METHOD.is_match(node) {
            return;
        }
        cx.add_offense_with_correction(key.source_range(), MSG, |corrector| {
            corrector.replace(key.source_range(), symbol_inspect(key_content));
        });
    }
}
