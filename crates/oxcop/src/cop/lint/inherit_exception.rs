//! Port of `lib/rubocop/cop/lint/inherit_exception.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::project_index_help::resolve_constant_in_index;
use crate::project_index::{Decl, DefKind, ProjectIndex};
use crate::cop::{Cop, Investigation};

static CLASS_NEW_CALL: NodePattern =
    NodePattern::new("(send (const {cbase nil?} :Class) :new $(const {cbase nil?} _))");

#[derive(Default)]
pub struct InheritException;

impl Cop for InheritException {
    fn name(&self) -> &'static str {
        "Lint/InheritException"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Class) {
            on_class(cx, node);
        } else {
            on_send(cx, node);
        }
    }
}

fn on_class<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(parent_class) = node.parent_class() else {
        return;
    };
    if exception_class(parent_class) {
        if inherit_exception_class_with_omitted_namespace(node) {
            return;
        }
        let preferred = preferred_base_class(cx);
        cx.add_offense_with_correction(parent_class.source_range(), &message(preferred), |corrector| {
            corrector.replace(parent_class.source_range(), preferred);
        });
    } else if let Some(via) = inherits_exception_via(cx, parent_class) {
        // No autocorrection: the `Exception` inheritance lives at another
        // class' definition site, possibly in another file.
        let message = format!(
            "Inherit from `{}` instead of `Exception` (inherited via `{via}`).",
            preferred_base_class(cx)
        );
        cx.add_offense(parent_class.source_range(), &message);
    }
}

/// `inherits_exception_via`: with the project index, indirect inheritance
/// is detected by walking the parent's indexed ancestry: `Exception` itself
/// is not indexed, so a chain ending in it shows up as an ancestor whose
/// superclass reference is unresolved and literally named `Exception`.
fn inherits_exception_via(cx: &Investigation<'_, '_>, parent_class: Node<'_>) -> Option<String> {
    let index = cx.project_index()?;
    if !parent_class.is(NodeType::Const) {
        return None;
    }
    let declaration = resolve_constant_in_index(&index, parent_class)?;
    if !declaration.is_class() {
        return None;
    }
    exception_ancestor(&index, declaration).map(|ancestor| index.name(ancestor))
}

fn exception_ancestor(index: &ProjectIndex, declaration: Decl) -> Option<Decl> {
    index
        .ancestors(declaration)
        .into_iter()
        .find(|ancestor| ancestor.is_class() && exception_superclass_reference(index, *ancestor))
}

fn exception_superclass_reference(index: &ProjectIndex, ancestor: Decl) -> bool {
    index.definitions(ancestor).into_iter().any(|definition| {
        definition.kind == DefKind::Class
            && index.superclass(definition).is_some_and(|superclass| {
                !superclass.resolved
                    && superclass.name.strip_prefix("::").unwrap_or(&superclass.name) == "Exception"
            })
    })
}

fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(constant) = CLASS_NEW_CALL.match_node(node).and_then(|c| c[0].node()) else {
        return;
    };
    if !exception_class(constant) {
        return;
    }
    let preferred = preferred_base_class(cx);
    cx.add_offense_with_correction(constant.source_range(), &message(preferred), |corrector| {
        corrector.replace(constant.source_range(), preferred);
    });
}

fn message(prefer: &str) -> String {
    format!("Inherit from `{prefer}` instead of `Exception`.")
}

fn exception_class(class_node: Node<'_>) -> bool {
    class_node.const_name().as_deref() == Some("Exception")
}

fn inherit_exception_class_with_omitted_namespace(class_node: Node<'_>) -> bool {
    let parent_class = class_node.parent_class().unwrap();
    if parent_class.namespace().is_some_and(|ns| ns.is(NodeType::Cbase)) {
        return false;
    }
    class_node
        .left_siblings()
        .into_iter()
        .any(|sibling| sibling.identifier().is_some_and(exception_class))
}

fn preferred_base_class(cx: &Investigation<'_, '_>) -> &'static str {
    match style(cx) {
        "runtime_error" => "RuntimeError",
        _ => "StandardError",
    }
}
