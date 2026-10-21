//! Port of `lib/rubocop/cop/lint/send_with_mixin_argument.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MIXIN_METHODS: &[&str] = &["include", "prepend", "extend"];

static SEND_WITH_MIXIN_ARGUMENT: NodePattern = NodePattern::new(
    "(send
       {nil? self (const _ _)} {:send :public_send :__send__}
       ({sym str} $#mixin_method?)
         $(const _ _)+)",
);

#[derive(Default)]
pub struct SendWithMixinArgument;

impl Cop for SendWithMixinArgument {
    fn name(&self) -> &'static str {
        "Lint/SendWithMixinArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["send", "public_send", "__send__"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
            name == "mixin_method?"
                && val
                    .sym()
                    .or(val.str())
                    .is_some_and(|m| MIXIN_METHODS.contains(&m))
        };
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        let Some(captures) = SEND_WITH_MIXIN_ARGUMENT.match_with(node, &ctx) else {
            return;
        };
        let Some(method) = captures[0].sym().or(captures[0].str()) else {
            return;
        };
        let module_names_source = captures[1]
            .nodes()
            .iter()
            .map(|n| n.source())
            .collect::<Vec<_>>()
            .join(", ");

        let bad_location = bad_location(node);
        let message = format!(
            "Use `{method} {module_names_source}` instead of `{}`.",
            cx.source(bad_location)
        );
        cx.add_offense_with_correction(bad_location, &message, |corrector| {
            corrector.replace(bad_location, format!("{method} {module_names_source}"));
        });
    }
}

fn bad_location(node: Node<'_>) -> Range {
    let loc = node.loc();
    range_between(
        loc.selector.unwrap_or_default().begin_pos,
        node.source_range().end_pos,
    )
}
