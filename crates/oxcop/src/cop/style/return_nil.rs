//! Port of `lib/rubocop/cop/style/return_nil.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const RETURN_MSG: &str = "Use `return` instead of `return nil`.";
const RETURN_NIL_MSG: &str = "Use `return nil` instead of `return`.";

static RETURN_NODE: NodePattern = NodePattern::new("(return)");
static RETURN_NIL_NODE: NodePattern = NodePattern::new("(return nil)");
static CHAINED_SEND: NodePattern = NodePattern::new("(send !nil? ...)");
static DEFINE_METHOD: NodePattern =
    NodePattern::new("(send _ {:define_method :define_singleton_method} _)");

#[derive(Default)]
pub struct ReturnNil;

impl Cop for ReturnNil {
    fn name(&self) -> &'static str {
        "Style/ReturnNil"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Return]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // Check Lint/NonLocalExitFromIterator first before this cop
        for n in node.ancestors() {
            if !(n.is(NodeType::Block) || n.is_any_def()) {
                continue;
            }
            if n.is_any_def() || n.is_lambda() {
                break;
            }
            let send_node = n.child(0).unwrap();
            if DEFINE_METHOD.is_match(send_node) {
                break;
            }
            if n.child(1).is_none_or(|args| args.children().is_empty()) {
                continue;
            }
            if CHAINED_SEND.is_match(send_node) {
                return;
            }
        }
        let style = style(cx);
        if (style == "return" && !RETURN_NIL_NODE.is_match(node))
            || (style == "return_nil" && !RETURN_NODE.is_match(node))
        {
            return;
        }
        let message = if style == "return" {
            RETURN_MSG
        } else {
            RETURN_NIL_MSG
        };
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            let corrected = if style == "return" {
                "return"
            } else {
                "return nil"
            };
            corrector.replace(node.source_range(), corrected);
        });
    }
}
