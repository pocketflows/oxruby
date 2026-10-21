//! Port of `lib/rubocop/cop/mixin/project_index_help.rb`: helpers for cops
//! that consult the project index (`cx.project_index()`).
//!
//! `external_dependency_checksum` (and the index signature behind it) only
//! feeds `ResultCache`, which oxcop doesn't have, so it isn't ported.

use std::path::Path;

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::project_index::{Decl, Def, DefKind, MixinKind, ProjectIndex};

/// `FILE_URI_PREFIX`.
pub const FILE_URI_PREFIX: &str = "file://";

/// `definitions_in_other_files`: the definitions that live in a file other
/// than the one being inspected, ordered by path and line. Definitions
/// without a `file://` URI (e.g. rubydex's built-in declarations) are
/// ignored.
pub fn definitions_in_other_files(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    definitions: &[Def],
) -> Vec<Def> {
    let current = cx.processed_source.file_path();
    let mut located: Vec<(Def, std::path::PathBuf, u32)> = definitions
        .iter()
        .filter_map(|def| {
            let location = index.location(*def)?;
            if !location.uri.starts_with(FILE_URI_PREFIX) {
                return None;
            }
            let path = location.file_path?;
            (!file_identical(&path, Path::new(&current))).then_some((
                *def,
                path,
                location.start_line,
            ))
        })
        .collect();
    located.sort_by(|a, b| (&a.1, a.2).cmp(&(&b.1, b.2)));
    located.into_iter().map(|(def, _, _)| def).collect()
}

/// `prior_definition_in_other_file`.
pub fn prior_definition_in_other_file(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    definitions: &[Def],
) -> Option<Def> {
    definitions_in_other_files(cx, index, definitions)
        .into_iter()
        .next()
}

/// `resolve_constant_in_index`: resolves a constant node the way Ruby does,
/// the first segment through the lexical nesting and every following
/// segment inside the previous one (`find_member`).
pub fn resolve_constant_in_index(index: &ProjectIndex, const_node: Node<'_>) -> Option<Decl> {
    let name = const_node.const_name()?;
    let mut segments = name.split("::");
    let first = segments.next()?;
    let nesting = if const_node.is_absolute() {
        Vec::new()
    } else {
        lexical_nesting_of(const_node)
    };
    let mut declaration = index.resolve_constant(first, &nesting);
    for segment in segments {
        let namespace = declaration.filter(|d| d.is_namespace())?;
        declaration = index.find_member(namespace, segment);
    }
    declaration
}

/// `lexical_nesting_of`: the lexical nesting the node's constants resolve
/// through, outermost first. Only scopes whose *body* contains the node
/// count.
pub fn lexical_nesting_of(node: Node<'_>) -> Vec<String> {
    let mut nesting = Vec::new();
    let mut child = node;
    for ancestor in node.ancestors() {
        if (ancestor.is(NodeType::Class) || ancestor.is(NodeType::Module))
            && ancestor.body().is_some_and(|body| body.id() == child.id())
        {
            nesting.push(
                ancestor
                    .identifier()
                    .and_then(|identifier| identifier.const_name())
                    .unwrap_or_default(),
            );
        }
        child = ancestor;
    }
    nesting.reverse();
    nesting
}

/// `indexed_singleton_of`: the declaration of `declaration`'s singleton
/// class, or `None` when no singleton method is defined on it anywhere in
/// the project.
pub fn indexed_singleton_of(index: &ProjectIndex, declaration: Decl) -> Option<Decl> {
    let name = index.name(declaration);
    let last = name.rsplit("::").next().unwrap_or(&name);
    index.get(&format!("{name}::<{last}>"))
}

/// `indexed_singleton_member`: the lookup starts from the first ancestor
/// that has a singleton-class declaration; its `find_member` covers the
/// rest of the chain.
pub fn indexed_singleton_member(
    index: &ProjectIndex,
    namespace: Decl,
    member_name: &str,
) -> Option<Decl> {
    for ancestor in index.ancestors(namespace) {
        if let Some(singleton) = indexed_singleton_of(index, ancestor) {
            return index.find_member(singleton, member_name);
        }
    }
    None
}

/// `inherited_index_member?`: whether an ancestor of `scope` other than
/// `scope` itself defines `member_name`.
pub fn inherited_index_member(index: &ProjectIndex, scope: Decl, member_name: &str) -> bool {
    let scope_name = index.name(scope);
    index.ancestors(scope).into_iter().any(|ancestor| {
        index.name(ancestor) != scope_name && index.member(ancestor, member_name).is_some()
    })
}

/// `fully_resolved_index_ancestry?`: no definition of any ancestor has an
/// unresolved superclass or mixin reference.
pub fn fully_resolved_index_ancestry(
    index: &ProjectIndex,
    declaration: Decl,
    ignore_extend: bool,
) -> bool {
    index.ancestors(declaration).into_iter().all(|ancestor| {
        index.definitions(ancestor).into_iter().all(|definition| {
            resolved_superclass_reference(index, definition)
                && resolved_mixin_references(index, definition, ignore_extend)
        })
    })
}

/// `resolved_superclass_reference?`.
pub fn resolved_superclass_reference(index: &ProjectIndex, definition: Def) -> bool {
    if definition.kind != DefKind::Class {
        return true;
    }
    index
        .superclass(definition)
        .is_none_or(|superclass| superclass.resolved)
}

/// `resolved_mixin_references?`.
pub fn resolved_mixin_references(
    index: &ProjectIndex,
    definition: Def,
    ignore_extend: bool,
) -> bool {
    if !definition.has_mixins() {
        return true;
    }
    !index
        .mixins(definition)
        .into_iter()
        .any(|mixin| !(ignore_extend && mixin.kind == MixinKind::Extend) && !mixin.resolved)
}

/// `same_file?`.
pub fn same_file(path: &Path, other: &Path) -> bool {
    if file_identical(path, other) {
        return true;
    }
    let normalize = |p: &Path| {
        crate::path_util::expand_path(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    normalize(path) == normalize(other)
}

/// `File.identical?`: both paths exist and are the same file.
#[cfg(unix)]
pub fn file_identical(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (Ok(a), Ok(b)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// `File.identical?`: both paths exist and are the same file.
#[cfg(not(unix))]
pub fn file_identical(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
