//! The project index (`Cop::Base#project_index`): a resolved `rubydex`
//! graph, with the part of the `rubydex` gem's Ruby API that RuboCop's cops
//! call (`Rubydex::Graph`, `Namespace`, `Definition`, `Signature`, ...).
//!
//! The gem is a C extension over the same `rubydex` crate
//! (`rubydex-sys/src/*_api.rs` and `ext/rubydex/*.c` at the pinned
//! commit); each method here follows the binding it stands for, on the
//! crate's safe API. Ruby's class checks become [`DeclKind`] / [`DefKind`]
//! predicates: `is_a?(Rubydex::Namespace)` is [`Decl::is_namespace`], and so
//! on.
//!
//! The graph sits behind a mutex: cops run on rayon's threads, and
//! `resolve_constant` needs the graph mutably (it registers the names it
//! looks up for the duration of the lookup, as the binding does).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rubydex::model::declaration::{Ancestor, Declaration, Namespace};
use rubydex::model::definitions::{Definition, Mixin as RdxMixin, Parameter};
use rubydex::model::graph::Graph;
use rubydex::model::ids::{
    ConstantReferenceId, DeclarationId, DefinitionId, NameId, StringId,
    declaration_id_from_lookup_name,
};
use rubydex::model::name::{NameRef, ParentScope};
use rubydex::model::visibility::Visibility as RdxVisibility;
use rubydex::resolution::Resolver;

/// `ProjectIndexHelp::BUILTIN_DOCUMENT_URI`.
pub const BUILTIN_DOCUMENT_URI: &str = "rubydex:built-in";

pub struct ProjectIndex {
    graph: Mutex<Graph>,
    /// Lint/UnusedPrivateMethod's `cached_reference_names`, which upstream
    /// keeps per index (one cache shared by every cop instance).
    pub reference_names: OnceLock<HashSet<String>>,
}

/// The Ruby class of a declaration (`rdxi_declaration_class_for_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    Class,
    Module,
    SingletonClass,
    Todo,
    Constant,
    ConstantAlias,
    Method,
    GlobalVariable,
    InstanceVariable,
    ClassVariable,
}

/// A `Rubydex::Declaration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decl {
    pub id: DeclarationId,
    pub kind: DeclKind,
}

impl Decl {
    fn new(id: DeclarationId, declaration: &Declaration) -> Self {
        let kind = match declaration {
            Declaration::Namespace(Namespace::Class(_)) => DeclKind::Class,
            Declaration::Namespace(Namespace::Module(_)) => DeclKind::Module,
            Declaration::Namespace(Namespace::SingletonClass(_)) => DeclKind::SingletonClass,
            Declaration::Namespace(Namespace::Todo(_)) => DeclKind::Todo,
            Declaration::Constant(_) => DeclKind::Constant,
            Declaration::ConstantAlias(_) => DeclKind::ConstantAlias,
            Declaration::Method(_) => DeclKind::Method,
            Declaration::GlobalVariable(_) => DeclKind::GlobalVariable,
            Declaration::InstanceVariable(_) => DeclKind::InstanceVariable,
            Declaration::ClassVariable(_) => DeclKind::ClassVariable,
        };
        Self { id, kind }
    }

    /// `is_a?(Rubydex::Namespace)` (`Todo` is a namespace too).
    pub fn is_namespace(self) -> bool {
        matches!(
            self.kind,
            DeclKind::Class | DeclKind::Module | DeclKind::SingletonClass | DeclKind::Todo
        )
    }

    /// `is_a?(Rubydex::Class)`.
    pub fn is_class(self) -> bool {
        self.kind == DeclKind::Class
    }

    /// `is_a?(Rubydex::Module)`.
    pub fn is_module(self) -> bool {
        self.kind == DeclKind::Module
    }
}

/// The Ruby class of a definition (`rdxi_definition_class_for_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefKind {
    Class,
    SingletonClass,
    Module,
    Constant,
    ConstantAlias,
    ConstantVisibility,
    MethodVisibility,
    Method,
    AttrAccessor,
    AttrReader,
    AttrWriter,
    GlobalVariable,
    InstanceVariable,
    ClassVariable,
    MethodAlias,
    GlobalVariableAlias,
}

/// A `Rubydex::Definition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Def {
    pub id: DefinitionId,
    pub kind: DefKind,
}

impl Def {
    fn new(id: DefinitionId, definition: &Definition) -> Self {
        let kind = match definition {
            Definition::Class(_) => DefKind::Class,
            Definition::SingletonClass(_) => DefKind::SingletonClass,
            Definition::Module(_) => DefKind::Module,
            Definition::Constant(_) => DefKind::Constant,
            Definition::ConstantAlias(_) => DefKind::ConstantAlias,
            Definition::ConstantVisibility(_) => DefKind::ConstantVisibility,
            Definition::MethodVisibility(_) => DefKind::MethodVisibility,
            Definition::Method(_) => DefKind::Method,
            Definition::AttrAccessor(_) => DefKind::AttrAccessor,
            Definition::AttrReader(_) => DefKind::AttrReader,
            Definition::AttrWriter(_) => DefKind::AttrWriter,
            Definition::GlobalVariable(_) => DefKind::GlobalVariable,
            Definition::InstanceVariable(_) => DefKind::InstanceVariable,
            Definition::ClassVariable(_) => DefKind::ClassVariable,
            Definition::MethodAlias(_) => DefKind::MethodAlias,
            Definition::GlobalVariableAlias(_) => DefKind::GlobalVariableAlias,
        };
        Self { id, kind }
    }

    /// `respond_to?(:mixins)`: class, module and singleton class
    /// definitions.
    pub fn has_mixins(self) -> bool {
        matches!(
            self.kind,
            DefKind::Class | DefKind::Module | DefKind::SingletonClass
        )
    }
}

/// `Rubydex::Location`: zero-based lines and columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    /// `to_file_path`, or `None` when the URI isn't a `file://` URI (where
    /// the gem raises `NotFileUriError`).
    pub file_path: Option<PathBuf>,
    pub start_line: u32,
    pub start_column: u32,
}

/// `Rubydex::Visibility#visibility`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

/// A `Rubydex::ConstantReference`: `resolved` is false for an
/// `UnresolvedConstantReference`, whose `name` is the referenced name's last
/// segment (`rdx_constant_reference_name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantReference {
    pub resolved: bool,
    pub name: String,
}

/// The kinds of `Rubydex::Mixin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixinKind {
    Include,
    Prepend,
    Extend,
}

/// A `Rubydex::Mixin`, with whether its constant reference resolved
/// (`constant_reference.is_a?(Rubydex::UnresolvedConstantReference)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mixin {
    pub kind: MixinKind,
    pub resolved: bool,
}

/// The kinds of `Rubydex::Signature::Parameter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterKind {
    Positional,
    OptionalPositional,
    RestPositional,
    Post,
    Keyword,
    OptionalKeyword,
    RestKeyword,
    Forward,
    Block,
}

/// A `Rubydex::Signature`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<ParameterKind>,
}

impl Signature {
    fn count(&self, kind: ParameterKind) -> usize {
        self.parameters.iter().filter(|p| **p == kind).count()
    }

    fn has(&self, kind: ParameterKind) -> bool {
        self.parameters.contains(&kind)
    }

    pub fn positional_parameters(&self) -> usize {
        self.count(ParameterKind::Positional)
    }

    pub fn optional_positional_parameters(&self) -> usize {
        self.count(ParameterKind::OptionalPositional)
    }

    pub fn rest_positional_parameter(&self) -> bool {
        self.has(ParameterKind::RestPositional)
    }

    pub fn post_parameters(&self) -> usize {
        self.count(ParameterKind::Post)
    }

    pub fn keyword_parameters(&self) -> usize {
        self.count(ParameterKind::Keyword)
    }

    pub fn optional_keyword_parameters(&self) -> usize {
        self.count(ParameterKind::OptionalKeyword)
    }

    pub fn rest_keyword_parameter(&self) -> bool {
        self.has(ParameterKind::RestKeyword)
    }

    pub fn forward_parameter(&self) -> bool {
        self.has(ParameterKind::Forward)
    }
}

impl ProjectIndex {
    fn new(graph: Graph) -> Self {
        Self {
            graph: Mutex::new(graph),
            reference_names: OnceLock::new(),
        }
    }

    /// `Graph.new`, `index_source(uri, source, 'ruby')` for each source in
    /// order, then `resolve` (the specs' `build_index`).
    pub fn from_sources(sources: &[(String, String)]) -> Self {
        let mut graph = Graph::new();
        for (uri, source) in sources {
            rubydex::indexing::index_source(
                &mut graph,
                uri.as_str().into(),
                source,
                &rubydex::indexing::LanguageId::Ruby,
            );
        }
        Resolver::new(&mut graph).resolve();
        Self::new(graph)
    }

    /// `Graph.new`, `index_all(paths)`, then `resolve`
    /// (`ProjectIndexLoader.build_index`). As `rdx_index_all`, the paths go
    /// through rubydex's file listing first, which keeps only indexable
    /// files (`.rb`, `.rbs`, ...); indexing errors are ignored, as the gem
    /// returns them without raising.
    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let mut graph = Graph::new();
        let paths = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let (file_paths, _errors) =
            rubydex::listing::collect_file_paths(paths, &graph.excluded_patterns());
        let _errors = rubydex::indexing::index_files(
            &mut graph,
            file_paths,
            rubydex::indexing::IndexerBackend::RubyIndexer,
        );
        Resolver::new(&mut graph).resolve();
        Self::new(graph)
    }

    fn graph(&self) -> MutexGuard<'_, Graph> {
        // A panic while the lock was held (a cop error) leaves the graph
        // as it was: lookups don't change it.
        self.graph.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `graph[name]`.
    pub fn get(&self, name: &str) -> Option<Decl> {
        let graph = self.graph();
        let id = declaration_id_from_lookup_name(name);
        graph.declarations().get(&id).map(|d| Decl::new(id, d))
    }

    /// `graph.resolve_constant(name, nesting)` (`rdx_graph_resolve_constant`
    /// with `name_from_nesting_names`).
    pub fn resolve_constant(&self, name: &str, nesting: &[String]) -> Option<Decl> {
        let mut graph = self.graph();
        let mut tracker = NameTracker {
            graph: &mut graph,
            tracked: Vec::new(),
        };
        let mut current_nesting = None;
        let mut current_name = ParentScope::None;
        for entry in nesting {
            tracker.process_qualified_name(entry, current_nesting, &mut current_name);
            current_nesting = current_name.as_ref().copied();
            current_name = ParentScope::None;
        }
        tracker.process_qualified_name(name, current_nesting, &mut current_name);
        let (ParentScope::Some(name_id) | ParentScope::Attached(name_id)) = current_name else {
            return None;
        };
        let id = Resolver::new(tracker.graph).resolve_constant(name_id)?;
        let declaration = tracker.graph.declarations().get(&id)?;
        Some(Decl::new(id, declaration))
    }

    fn with_declaration<R>(
        &self,
        decl: Decl,
        f: impl FnOnce(&Graph, &Declaration) -> R,
    ) -> Option<R> {
        let graph = self.graph();
        let declaration = graph.declarations().get(&decl.id)?;
        Some(f(&graph, declaration))
    }

    fn with_namespace<R>(&self, decl: Decl, f: impl FnOnce(&Graph, &Namespace) -> R) -> Option<R> {
        let graph = self.graph();
        let namespace = graph.declarations().get(&decl.id)?.as_namespace()?;
        Some(f(&graph, namespace))
    }

    /// `Declaration#name`.
    pub fn name(&self, decl: Decl) -> String {
        self.with_declaration(decl, |_, d| d.name().to_owned())
            .unwrap_or_default()
    }

    /// `Declaration#definitions`.
    pub fn definitions(&self, decl: Decl) -> Vec<Def> {
        self.with_declaration(decl, |graph, d| {
            d.definitions()
                .iter()
                .filter_map(|id| graph.definitions().get(id).map(|def| Def::new(*id, def)))
                .collect()
        })
        .unwrap_or_default()
    }

    /// `Declaration#owner`.
    pub fn owner(&self, decl: Decl) -> Option<Decl> {
        self.with_declaration(decl, |graph, d| {
            let owner_id = *d.owner_id();
            graph
                .declarations()
                .get(&owner_id)
                .map(|owner| Decl::new(owner_id, owner))
        })
        .flatten()
    }

    /// `Namespace#member(name)`.
    pub fn member(&self, namespace: Decl, member: &str) -> Option<Decl> {
        self.with_namespace(namespace, |graph, ns| {
            let id = *ns.member(&StringId::from(member))?;
            graph.declarations().get(&id).map(|d| Decl::new(id, d))
        })
        .flatten()
    }

    /// `Namespace#find_member(name)`: the member on the namespace or the
    /// first ancestor that has it.
    pub fn find_member(&self, namespace: Decl, member: &str) -> Option<Decl> {
        let graph = self.graph();
        let id = rubydex::query::find_member_in_ancestors(
            &graph,
            namespace.id,
            StringId::from(member),
            false,
        )
        .ok()?;
        graph.declarations().get(&id).map(|d| Decl::new(id, d))
    }

    /// `Namespace#members`.
    pub fn members(&self, namespace: Decl) -> Vec<Decl> {
        self.with_namespace(namespace, |graph, ns| {
            ns.members()
                .values()
                .filter_map(|id| graph.declarations().get(id).map(|d| Decl::new(*id, d)))
                .collect()
        })
        .unwrap_or_default()
    }

    /// `Namespace#ancestors`: the resolved ones (unresolved links are left
    /// out, as `rdx_declaration_ancestors` does).
    pub fn ancestors(&self, namespace: Decl) -> Vec<Decl> {
        self.with_namespace(namespace, |graph, ns| {
            ns.ancestors()
                .iter()
                .filter_map(|ancestor| match ancestor {
                    Ancestor::Complete(id) => {
                        graph.declarations().get(id).map(|d| Decl::new(*id, d))
                    }
                    Ancestor::Partial(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// `Namespace#descendants`.
    pub fn descendants(&self, namespace: Decl) -> Vec<Decl> {
        self.with_namespace(namespace, |graph, ns| {
            ns.descendants()
                .iter()
                .filter_map(|id| graph.declarations().get(id).map(|d| Decl::new(*id, d)))
                .collect()
        })
        .unwrap_or_default()
    }

    /// `visibility` (classes, modules, constants and methods; `None`
    /// elsewhere, where the gem has no such method). A `module_function`
    /// method definition, which the gem can't translate (its binding hits
    /// `unimplemented!`), counts as private: that's the visibility of the
    /// instance method it defines.
    pub fn visibility(&self, decl: Decl) -> Option<Visibility> {
        let graph = self.graph();
        Some(match graph.visibility(&decl.id)? {
            RdxVisibility::Public => Visibility::Public,
            RdxVisibility::Protected => Visibility::Protected,
            RdxVisibility::Private | RdxVisibility::ModuleFunction => Visibility::Private,
        })
    }

    /// `public?`.
    pub fn is_public(&self, decl: Decl) -> bool {
        self.visibility(decl) == Some(Visibility::Public)
    }

    /// `private?`.
    pub fn is_private(&self, decl: Decl) -> bool {
        self.visibility(decl) == Some(Visibility::Private)
    }

    fn with_definition<R>(&self, def: Def, f: impl FnOnce(&Graph, &Definition) -> R) -> Option<R> {
        let graph = self.graph();
        let definition = graph.definitions().get(&def.id)?;
        Some(f(&graph, definition))
    }

    /// `Definition#location`.
    pub fn location(&self, def: Def) -> Option<Location> {
        self.with_definition(def, |graph, definition| {
            let document = graph.documents().get(definition.uri_id())?;
            let location = definition.offset().to_location(document);
            Some(Location {
                uri: document.uri().to_owned(),
                file_path: document.file_path(),
                start_line: location.start_line(),
                start_column: location.start_col(),
            })
        })
        .flatten()
    }

    /// `Definition#deprecated?`.
    pub fn is_deprecated(&self, def: Def) -> bool {
        self.with_definition(def, |_, definition| definition.is_deprecated())
            .unwrap_or(false)
    }

    /// `Definition#comments.map(&:string)`.
    pub fn comments(&self, def: Def) -> Vec<String> {
        self.with_definition(def, |_, definition| {
            definition
                .comments()
                .iter()
                .map(|comment| comment.string().to_owned())
                .collect()
        })
        .unwrap_or_default()
    }

    /// `ClassDefinition#superclass`.
    pub fn superclass(&self, def: Def) -> Option<ConstantReference> {
        self.with_definition(def, |graph, definition| match definition {
            Definition::Class(class) => class
                .superclass_ref()
                .map(|id| constant_reference(graph, *id)),
            _ => None,
        })
        .flatten()
    }

    /// `#mixins` of a class, module or singleton class definition.
    pub fn mixins(&self, def: Def) -> Vec<Mixin> {
        self.with_definition(def, |graph, definition| {
            let mixins = match definition {
                Definition::Class(d) => d.mixins(),
                Definition::Module(d) => d.mixins(),
                Definition::SingletonClass(d) => d.mixins(),
                _ => return Vec::new(),
            };
            mixins
                .iter()
                .map(|mixin| Mixin {
                    kind: match mixin {
                        RdxMixin::Include(_) => MixinKind::Include,
                        RdxMixin::Prepend(_) => MixinKind::Prepend,
                        RdxMixin::Extend(_) => MixinKind::Extend,
                    },
                    resolved: constant_reference(graph, *mixin.constant_reference_id()).resolved,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// `MethodDefinition#signatures` (and `MethodAliasDefinition#signatures`,
    /// the signatures of the alias's target).
    pub fn signatures(&self, def: Def) -> Vec<Signature> {
        self.with_definition(def, |graph, definition| match definition {
            Definition::Method(method) => method_signatures(method),
            Definition::MethodAlias(_) => {
                let Ok(target) = rubydex::query::follow_method_alias(graph, def.id) else {
                    return Vec::new();
                };
                let Some(Declaration::Method(method)) = graph.declarations().get(&target) else {
                    return Vec::new();
                };
                method
                    .definitions()
                    .iter()
                    .filter_map(|id| match graph.definitions().get(id) {
                        Some(Definition::Method(method)) => Some(method_signatures(method)),
                        _ => None,
                    })
                    .flatten()
                    .collect()
            }
            _ => Vec::new(),
        })
        .unwrap_or_default()
    }

    /// `graph.method_references.map(&:name)`.
    pub fn method_reference_names(&self) -> Vec<String> {
        let graph = self.graph();
        graph
            .method_references()
            .values()
            .filter_map(|reference| graph.strings().get(reference.str()))
            .map(|name| name.as_str().to_owned())
            .collect()
    }

    /// `graph.documents.map(&:uri)`.
    pub fn document_uris(&self) -> Vec<String> {
        let graph = self.graph();
        graph
            .documents()
            .values()
            .map(|document| document.uri().to_owned())
            .collect()
    }
}

fn constant_reference(graph: &Graph, id: ConstantReferenceId) -> ConstantReference {
    let name = graph
        .constant_references()
        .get(&id)
        .and_then(|reference| graph.names().get(reference.name_id()));
    ConstantReference {
        resolved: name.is_some_and(|name| matches!(name, NameRef::Resolved(_))),
        name: name
            .and_then(|name| graph.strings().get(name.str()))
            .map(|string| string.as_str().to_owned())
            .unwrap_or_default(),
    }
}

fn method_signatures(method: &rubydex::model::definitions::MethodDefinition) -> Vec<Signature> {
    method
        .signatures()
        .as_slice()
        .iter()
        .map(|signature| Signature {
            parameters: signature
                .iter()
                .map(|parameter| match parameter {
                    Parameter::RequiredPositional(_) => ParameterKind::Positional,
                    Parameter::OptionalPositional(_) => ParameterKind::OptionalPositional,
                    Parameter::RestPositional(_) => ParameterKind::RestPositional,
                    Parameter::Post(_) => ParameterKind::Post,
                    Parameter::RequiredKeyword(_) => ParameterKind::Keyword,
                    Parameter::OptionalKeyword(_) => ParameterKind::OptionalKeyword,
                    Parameter::RestKeyword(_) => ParameterKind::RestKeyword,
                    Parameter::Forward(_) => ParameterKind::Forward,
                    Parameter::Block(_) => ParameterKind::Block,
                })
                .collect(),
        })
        .collect()
}

/// `name_api::NameTracker`: names registered for one lookup, untracked
/// again when it ends.
struct NameTracker<'g> {
    graph: &'g mut Graph,
    tracked: Vec<NameId>,
}

impl NameTracker<'_> {
    fn track_name(
        &mut self,
        string: &str,
        parent_scope: ParentScope,
        nesting: Option<NameId>,
    ) -> NameId {
        let str_id = self.graph.intern_string(string.to_owned());
        let name_id = self.graph.add_name(str_id, parent_scope, nesting);
        self.tracked.push(name_id);
        name_id
    }

    /// `name_api::process_qualified_name`.
    fn process_qualified_name(
        &mut self,
        qualified_name: &str,
        current_nesting: Option<NameId>,
        current_name: &mut ParentScope,
    ) {
        for part in qualified_name.split("::") {
            if part.is_empty() {
                *current_name = ParentScope::TopLevel;
                continue;
            }
            let (parent_scope, nesting_for_part) = if part.starts_with('<') {
                let attached_id = match *current_name {
                    ParentScope::Some(id) | ParentScope::Attached(id) => Some(id),
                    _ => current_nesting,
                };
                (
                    attached_id.map_or(ParentScope::None, ParentScope::Attached),
                    attached_id,
                )
            } else {
                (*current_name, current_nesting)
            };
            let name_id = self.track_name(part, parent_scope, nesting_for_part);
            *current_name = ParentScope::Some(name_id);
        }
    }
}

impl Drop for NameTracker<'_> {
    fn drop(&mut self) {
        for name_id in self.tracked.drain(..) {
            self.graph.untrack_name(name_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(sources: &[(&str, &str)]) -> ProjectIndex {
        let sources: Vec<(String, String)> = sources
            .iter()
            .map(|(uri, source)| ((*uri).to_owned(), (*source).to_owned()))
            .collect();
        ProjectIndex::from_sources(&sources)
    }

    #[test]
    fn resolves_constants_through_the_nesting() {
        let index = index(&[(
            "file:///a.rb",
            "module Outer\n  class Inner; end\nend\nclass Inner; end\n",
        )]);
        let nested = index
            .resolve_constant("Inner", &["Outer".to_owned()])
            .unwrap();
        assert_eq!(index.name(nested), "Outer::Inner");
        assert!(nested.is_class());
        let top = index.resolve_constant("Inner", &[]).unwrap();
        assert_eq!(index.name(top), "Inner");
        assert!(index.resolve_constant("Missing", &[]).is_none());
        // Lookups leave the graph as it was.
        assert_eq!(
            index.resolve_constant("Inner", &["Outer".to_owned()]),
            Some(nested)
        );
    }

    #[test]
    fn members_ancestors_and_definitions() {
        let index = index(&[
            (
                "file:///base.rb",
                "class Base\n  def run(a, b = 1, *rest, k:); end\nend\n",
            ),
            (
                "file:///child.rb",
                "class Child < Base\n  # @deprecated Use x.\n  def go; end\nend\n",
            ),
        ]);
        let child = index.get("Child").unwrap();
        let names: Vec<String> = index
            .ancestors(child)
            .into_iter()
            .map(|a| index.name(a))
            .collect();
        assert_eq!(&names[..2], ["Child", "Base"]);
        let run = index.find_member(child, "run()").unwrap();
        assert!(index.member(child, "run()").is_none());
        let def = index.definitions(run)[0];
        assert_eq!(def.kind, DefKind::Method);
        let signature = &index.signatures(def)[0];
        assert_eq!(signature.positional_parameters(), 1);
        assert_eq!(signature.optional_positional_parameters(), 1);
        assert!(signature.rest_positional_parameter());
        assert_eq!(signature.keyword_parameters(), 1);
        let location = index.location(def).unwrap();
        assert_eq!(location.start_line, 1);
        assert_eq!(location.file_path, Some(PathBuf::from("/base.rb")));
        let go = index.member(child, "go()").unwrap();
        let go_def = index.definitions(go)[0];
        assert!(index.is_deprecated(go_def));
        assert!(index.is_public(go));
        let base = index.get("Base").unwrap();
        let descendants: Vec<String> = index
            .descendants(base)
            .into_iter()
            .map(|d| index.name(d))
            .collect();
        assert!(descendants.contains(&"Child".to_owned()));
        let child_def = index.definitions(child)[0];
        assert!(index.superclass(child_def).unwrap().resolved);
    }
}
