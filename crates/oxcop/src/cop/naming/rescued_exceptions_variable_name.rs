//! Port of `lib/rubocop/cop/naming/rescued_exceptions_variable_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct RescuedExceptionsVariableName;

impl Cop for RescuedExceptionsVariableName {
    fn name(&self) -> &'static str {
        "Naming/RescuedExceptionsVariableName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(offending_name) = variable_name(node) else {
            return;
        };
        // Handle nested rescues by only requiring the outer one to use the
        // configured variable name, so that nested rescues don't use the
        // same variable.
        if node.ancestors().any(|a| a.is(NodeType::Resbody)) {
            return;
        }
        let preferred_name = preferred_name(cx, offending_name);
        if preferred_name == offending_name {
            return;
        }
        // check variable shadowing for exception variable
        if shadowed_variable_name(cx, node) {
            return;
        }
        let range = node.exception_variable().unwrap().source_range();
        let message = format!("Use `{preferred_name}` instead of `{offending_name}`.");
        cx.add_offense_with_correction(range, &message, |corrector| {
            autocorrect(corrector, node, range, offending_name, &preferred_name);
        });
    }
}

fn autocorrect(
    corrector: &mut Corrector,
    node: Node<'_>,
    range: Range,
    offending_name: &str,
    preferred_name: &str,
) {
    corrector.replace(range, preferred_name);
    // Once the exception variable is reassigned, later references point to
    // a different value, so stop correcting after the reassignment - both in
    // the body and in the code following the `begin`/`rescue`.
    if correct_node(corrector, node.child(2), offending_name, preferred_name) {
        return;
    }
    let Some(kwbegin_node) = node
        .parent()
        .and_then(|p| p.ancestors().find(|a| a.is(NodeType::Kwbegin)))
    else {
        return;
    };
    for child_node in kwbegin_node.right_siblings() {
        if correct_node(corrector, Some(child_node), offending_name, preferred_name) {
            break;
        }
    }
}

fn variable_name_matches(node: Node<'_>, name: &str) -> bool {
    if node.is(NodeType::Masgn) {
        node.descendants()
            .filter(|d| d.is(NodeType::Lvasgn))
            .any(|lvasgn| variable_name_matches(lvasgn, name))
    } else {
        node.name() == Some(name)
    }
}

/// Returns true once the exception variable is reassigned (a signal to stop
/// correcting later references).
fn correct_node(
    corrector: &mut Corrector,
    node: Option<Node<'_>>,
    offending_name: &str,
    preferred_name: &str,
) -> bool {
    let Some(node) = node else {
        return false;
    };
    for child_node in node
        .each_node()
        .filter(|n| matches!(n.ty(), NodeType::Lvar | NodeType::Lvasgn | NodeType::Masgn))
    {
        if !variable_name_matches(child_node, offending_name) {
            continue;
        }
        if child_node.is(NodeType::Lvar) {
            match child_node.parent() {
                Some(parent) if parent.is_value_omission() => {
                    corrector.insert_after(parent.loc().operator.unwrap(), format!(" {preferred_name}"));
                }
                _ => corrector.replace(child_node.source_range(), preferred_name),
            }
        }
        if matches!(child_node.ty(), NodeType::Masgn | NodeType::Lvasgn) {
            // If the exception variable is reassigned, that assignment needs
            // to be corrected. Further `lvar` nodes will not be corrected
            // though since they now refer to a different variable.
            correct_node(corrector, child_node.rhs(), offending_name, preferred_name);
            return true;
        }
    }
    false
}

fn preferred_name(cx: &Investigation<'_, '_>, variable_name: &str) -> String {
    let preferred_name = cx.cop_config_str("PreferredName").unwrap_or("e");
    if variable_name.starts_with('_') {
        format!("_{preferred_name}")
    } else {
        preferred_name.to_owned()
    }
}

fn variable_name(node: Node<'_>) -> Option<&str> {
    node.exception_variable().and_then(|v| v.name())
}

fn shadowed_variable_name(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    // `preferred_name(n)` with a node: its `to_s` never starts with `_`.
    let preferred = preferred_name(cx, "");
    node.descendants()
        .filter(|n| n.is(NodeType::Lvar))
        .any(|n| n.name() == Some(preferred.as_str()))
}
