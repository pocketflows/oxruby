//! Port of `lib/rubocop/cop/lint/unused_private_method.rb`.

use std::collections::HashSet;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::project_index_help::resolve_constant_in_index;
use crate::cop::{Cop, Investigation};
use crate::project_index::{Decl, ProjectIndex};

/// Methods invoked implicitly by the Ruby runtime.
const IMPLICITLY_INVOKED_METHODS: &[&str] = &[
    "initialize",
    "initialize_copy",
    "initialize_clone",
    "initialize_dup",
    "method_missing",
    "respond_to_missing?",
    "marshal_dump",
    "marshal_load",
    "coerce",
    "instance_variables_to_inspect",
    "inherited",
    "included",
    "extended",
    "prepended",
    "append_features",
    "extend_object",
    "prepend_features",
    "const_missing",
    "const_added",
    "method_added",
    "method_removed",
    "method_undefined",
    "singleton_method_added",
    "singleton_method_removed",
    "singleton_method_undefined",
];

#[derive(Default)]
pub struct UnusedPrivateMethod {
    literal_names: Option<HashSet<String>>,
    literal_name_prefixes: Option<HashSet<String>>,
}

impl Cop for UnusedPrivateMethod {
    fn name(&self) -> &'static str {
        "Lint/UnusedPrivateMethod"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.literal_names = None;
        self.literal_name_prefixes = None;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = cx.project_index() else {
            return;
        };
        let Some(method_name) = node.method_name() else {
            return;
        };
        let Some(declaration) = checkable_declaration(cx, &index, node, method_name) else {
            return;
        };
        if self.referenced(cx, &index, method_name)
            || owner_with_descendants(&index, declaration)
            || overrides(&index, declaration, method_name)
        {
            return;
        }
        let (Some(keyword), Some(name)) = (node.loc().keyword, node.loc().name) else {
            return;
        };
        cx.add_offense(
            keyword.join(name),
            &format!("Private method `{method_name}` appears to be unused."),
        );
    }
}

impl UnusedPrivateMethod {
    fn referenced(&mut self, cx: &Investigation<'_, '_>, index: &ProjectIndex, name: &str) -> bool {
        referenced_names(index).contains(name)
            || self.literal_names(cx).contains(name)
            || self
                .literal_name_prefixes(cx)
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
    }

    /// `literal_names`: symbol literals and identifier-like tokens inside
    /// string literals in the current file.
    fn literal_names(&mut self, cx: &Investigation<'_, '_>) -> &HashSet<String> {
        self.literal_names.get_or_insert_with(|| {
            let mut names = HashSet::new();
            let Some(root) = cx.processed_source.ast() else {
                return names;
            };
            let identifier = crate::regex!(r"[a-zA-Z_]\w*[?!=]?");
            for literal in root.descendants() {
                if let Some(value) = literal.sym_value() {
                    names.insert(value.to_owned());
                } else if literal.is(NodeType::Str) {
                    let value = match literal.value() {
                        Some(Child::Str(s)) => s.clone(),
                        Some(Child::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                        _ => continue,
                    };
                    for (start, end) in identifier.find_iter(&value) {
                        names.insert(value[start..end].to_owned());
                    }
                }
            }
            names
        })
    }

    /// `literal_name_prefixes`: the leading literal fragment of an
    /// interpolated symbol or string acts as a method-name prefix.
    fn literal_name_prefixes(&mut self, cx: &Investigation<'_, '_>) -> &HashSet<String> {
        self.literal_name_prefixes.get_or_insert_with(|| {
            let Some(root) = cx.processed_source.ast() else {
                return HashSet::new();
            };
            root.descendants()
                .filter(|node| node.is(NodeType::Dsym) || node.is(NodeType::Dstr))
                .filter_map(interpolation_prefix)
                .collect()
        })
    }
}

/// `checkable_declaration`.
fn checkable_declaration(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    node: Node<'_>,
    method_name: &str,
) -> Option<Decl> {
    if IMPLICITLY_INVOKED_METHODS.contains(&method_name) || allowed_name(cx, method_name) {
        return None;
    }
    if node
        .ancestors()
        .any(|a| a.is_any_def() || a.is_any_block() || a.is(NodeType::Sclass))
    {
        return None;
    }
    let namespace_node = node
        .ancestors()
        .find(|a| a.is(NodeType::Class) || a.is(NodeType::Module))?;
    let namespace = resolve_constant_in_index(index, namespace_node.identifier()?)?;
    if !namespace.is_namespace() {
        return None;
    }
    let declaration = index.member(namespace, &format!("{method_name}()"))?;
    index.is_private(declaration).then_some(declaration)
}

/// `allowed_name?`.
fn allowed_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    cx.cop_config_list("AllowedNames").contains(&name) || matches_allowed_pattern(cx, name)
}

/// `referenced_names`: every method name referenced in the project,
/// computed once per index.
fn referenced_names(index: &ProjectIndex) -> &HashSet<String> {
    index.reference_names.get_or_init(|| {
        index
            .method_reference_names()
            .into_iter()
            .map(|name| name.strip_suffix("()").map(str::to_owned).unwrap_or(name))
            .collect()
    })
}

/// `interpolation_prefix`.
fn interpolation_prefix(node: Node<'_>) -> Option<String> {
    if node.child_nodes().all(|child| child.is(NodeType::Str)) {
        return None;
    }
    let leading = node.child_nodes().next()?;
    if !leading.is(NodeType::Str) {
        return None;
    }
    let value = match leading.value() {
        Some(Child::Str(s)) => s.clone(),
        Some(Child::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => return None,
    };
    let prefix = crate::regex!(r"[a-zA-Z_]\w*\z");
    let (start, end) = prefix.find(&value)?;
    Some(value[start..end].to_owned())
}

/// `owner_with_descendants?`: a method defined in a class or module with
/// descendants may be invoked through `super` or inherited dispatch.
fn owner_with_descendants(index: &ProjectIndex, declaration: Decl) -> bool {
    let Some(owner) = index.owner(declaration).filter(|o| o.is_namespace()) else {
        return false;
    };
    let name = index.name(owner);
    index
        .descendants(owner)
        .into_iter()
        .any(|descendant| index.name(descendant) != name)
}

/// `override?`: an override of an inherited method may be invoked
/// polymorphically.
fn overrides(index: &ProjectIndex, declaration: Decl, method_name: &str) -> bool {
    let Some(owner) = index.owner(declaration).filter(|o| o.is_namespace()) else {
        return false;
    };
    let name = index.name(owner);
    let member_name = format!("{method_name}()");
    index.ancestors(owner).into_iter().any(|ancestor| {
        index.name(ancestor) != name && index.member(ancestor, &member_name).is_some()
    })
}
