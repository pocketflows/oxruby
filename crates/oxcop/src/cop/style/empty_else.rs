//! Port of `lib/rubocop/cop/style/empty_else.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::on_normal_if_unless::normal_if_unless;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "Redundant `else`-clause.";

#[derive(Default)]
pub struct EmptyElse;

impl Cop for EmptyElse {
    fn name(&self) -> &'static str {
        "Style/EmptyElse"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::If) && !normal_if_unless(node) {
            return;
        }
        check(cx, node);
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let ps = cx.processed_source;
    if cx.cop_config_bool("AllowComments", false) && comment_in_else(ps, node) {
        return;
    }
    let style = style(cx);
    let forbidden = autocorrect_forbidden(cx, node.ty().name());
    if matches!(style, "empty" | "both") && node.has_else() && node.else_branch().is_none() {
        add(cx, node, forbidden);
    }
    if matches!(style, "nil" | "both") && node.else_branch().is_some_and(|e| e.is(NodeType::Nil)) {
        add(cx, node, forbidden);
    }
}

fn add(cx: &mut Investigation<'_, '_>, node: Node<'_>, forbidden: bool) {
    let ps = cx.processed_source;
    cx.add_offense_with_correction(node.loc().else_.unwrap(), MSG, |corrector| {
        autocorrect(ps, corrector, node, forbidden);
    });
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>, forbidden: bool) {
    if forbidden || comment_in_else(ps, node) {
        return;
    }
    let Some(end) = base_node(node).loc().end else {
        return;
    };
    corrector.remove(range_between(
        node.loc().else_.unwrap().begin_pos,
        end.begin_pos,
    ));
}

fn comment_in_else(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let mut node = node;
    while node.is(NodeType::If) && node.is_elsif() {
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    let Some(else_loc) = node.loc().else_ else {
        return false;
    };
    ps.contains_comment(else_loc.join(node.source_range().end()))
}

fn base_node(node: Node<'_>) -> Node<'_> {
    if node.is(NodeType::Case) || !node.is_elsif() {
        return node;
    }
    node.ancestors()
        .filter(|a| matches!(a.ty(), NodeType::If | NodeType::Case | NodeType::When))
        .find(|a| a.loc().end.is_some())
        .unwrap_or(node)
}

/// `autocorrect_forbidden?(type)`: `Style/MissingElse` would ask for the
/// `else` back.
fn autocorrect_forbidden(cx: &Investigation<'_, '_>, ty: &str) -> bool {
    let missing_cfg = cx.config.for_cop("Style/MissingElse");
    let enabled = match missing_cfg.get("Enabled") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(_) => true,
    };
    if !enabled {
        return false;
    }
    missing_cfg
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .is_some_and(|s| s == ty || s == "both")
}
