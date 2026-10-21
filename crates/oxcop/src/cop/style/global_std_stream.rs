//! Port of `lib/rubocop/cop/style/global_std_stream.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const STD_STREAMS: &[&str] = &["STDIN", "STDOUT", "STDERR"];

static CONST_TO_GVAR_ASSIGNMENT: NodePattern = NodePattern::new("(gvasgn %1 (const nil? _))");

#[derive(Default)]
pub struct GlobalStdStream;

impl Cop for GlobalStdStream {
    fn name(&self) -> &'static str {
        "Style/GlobalStdStream"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if namespaced(node) {
            return;
        }
        let Some(const_name) = node.short_name().filter(|n| STD_STREAMS.contains(n)) else {
            return;
        };
        let gvar_name = format!("${}", const_name.to_lowercase());
        if let Some(parent) = node.parent() {
            let ctx = MatchContext {
                params: &[Param::Sym(&gvar_name)],
                ..Default::default()
            };
            if CONST_TO_GVAR_ASSIGNMENT.match_with(parent, &ctx).is_some() {
                return;
            }
        }
        let message = format!("Use `{gvar_name}` instead of `{const_name}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), &gvar_name);
        });
    }
}

fn namespaced(node: Node<'_>) -> bool {
    node.namespace()
        .is_some_and(|ns| !node.is_absolute() || !ns.is(NodeType::Cbase))
}
