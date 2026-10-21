//! Port of `lib/rubocop/cop/lint/raise_exception.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `StandardError` over `Exception`.";

static EXCEPTION: NodePattern =
    NodePattern::new("(send nil? {:raise :fail} $(const ${cbase nil?} :Exception) ... )");
static EXCEPTION_NEW_WITH_MESSAGE: NodePattern = NodePattern::new(
    "(send nil? {:raise :fail}
       (send $(const ${cbase nil?} :Exception) :new ... ))",
);

#[derive(Default)]
pub struct RaiseException;

impl Cop for RaiseException {
    fn name(&self) -> &'static str {
        "Lint/RaiseException"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["raise", "fail"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let captures = EXCEPTION
            .match_node(node)
            .or_else(|| EXCEPTION_NEW_WITH_MESSAGE.match_node(node));
        let Some(captures) = captures else {
            return;
        };
        let Some(exception_class) = captures[0].node() else {
            return;
        };
        check(cx, node, exception_class, captures[1].is_nil());
    }
}

fn check<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    exception_class: Node<'a>,
    cbase_nil: bool,
) {
    if cbase_nil && implicit_namespace(cx, node) {
        return;
    }

    cx.add_offense_with_correction(exception_class.source_range(), MSG, |corrector| {
        let prefer_exception = if exception_class
            .child(0)
            .is_some_and(|c| c.is(NodeType::Cbase))
        {
            "::StandardError"
        } else {
            "StandardError"
        };
        corrector.replace(exception_class.source_range(), prefer_exception);
    });
}

fn implicit_namespace(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.is(NodeType::Module)
        && let Some(identifier) = parent.identifier()
    {
        let namespace = identifier.source();
        if cx
            .cop_config_list("AllowedImplicitNamespaces")
            .contains(&namespace)
        {
            return true;
        }
    }
    implicit_namespace(cx, parent)
}
