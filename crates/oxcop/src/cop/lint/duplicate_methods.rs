//! Port of `lib/rubocop/cop/lint/duplicate_methods.rb`.
//!
//! Definitions are kept for the cop instance's lifetime, as `@definitions`
//! is.

use std::collections::{HashMap, HashSet};

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::project_index_help::definitions_in_other_files;
use crate::cop::{Cop, Investigation};
use crate::project_index::{Def, DefKind, ProjectIndex};
use crate::path_util::{match_path, smart_path};
use crate::processed_source::Range;
use crate::regex;

static METHOD_ALIAS: NodePattern = NodePattern::new("(alias (sym $_name) (sym $_original_name))");
static ALIAS_METHOD: NodePattern =
    NodePattern::new("(send nil? :alias_method (sym $_name) (sym $_original_name))");
static DELEGATE_ARGS: NodePattern = NodePattern::new(
    "(send nil? _
      ({sym str} $_)+
      (hash <(pair (sym :to) {sym str}) ...>)
    )",
);
static DELEGATOR: NodePattern = NodePattern::new(
    "(send nil? {:def_delegator :def_instance_delegator}
      {
        {sym str} ({sym str} $_) |
        {sym str} {sym str} ({sym str} $_)
      }
    )",
);
static DELEGATORS: NodePattern = NodePattern::new(
    "(send nil? {:def_delegators :def_instance_delegators}
      {sym str}
      ({sym str} $_)+
    )",
);
static ACTIVE_SUPPORT_REDEFINITION_MARKER: NodePattern = NodePattern::new(
    "(send nil? {:silence_redefinition_of_method :redefine_method} ({sym str} $_) ...)",
);
static SYM_NAME: NodePattern = NodePattern::new("(sym $_name)");
static CLASS_OR_MODULE_NEW_BLOCK: NodePattern =
    NodePattern::new("(block (send (const _ {:Class :Module}) :new ...) ...)");
static CLASS_NEW_BLOCK: NodePattern = NodePattern::new("(block (send (const _ :Class) :new ...) ...)");

/// Where a definition was seen (`@definitions[key]`): its file and line.
#[derive(Debug, Clone)]
struct Definition {
    path: String,
    line: usize,
}

#[derive(Default)]
pub struct DuplicateMethods {
    definitions: HashMap<String, Definition>,
    scopes: HashMap<NodeType, Vec<String>>,
    intentionally_redefined: HashSet<String>,
}

impl Cop for DuplicateMethods {
    fn name(&self) -> &'static str {
        "Lint/DuplicateMethods"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs, NodeType::Alias, NodeType::Send]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.intentionally_redefined.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Def => {
                if inside_condition(node) {
                    return;
                }
                self.found_instance_method(cx, node, node.method_name().unwrap_or_default());
            }
            NodeType::Defs => self.on_defs(cx, node),
            NodeType::Alias => self.on_alias(cx, node),
            _ => self.on_send(cx, node),
        }
    }
}

fn sym_value<'a>(val: Option<crate::ast::node_pattern::Val<'a>>) -> Option<&'a str> {
    let val = val?;
    val.sym().or_else(|| val.str())
}

impl DuplicateMethods {
    fn on_defs<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if inside_condition(node) {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let name = node.method_name().unwrap_or_default();
        if receiver.is(NodeType::Const) {
            let const_name = receiver.child_sym(1).unwrap_or_default();
            self.check_const_receiver(cx, node, name, const_name);
        } else if receiver.is(NodeType::SelfNode) {
            self.check_self_receiver(cx, node, name);
        }
    }

    fn on_alias<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = METHOD_ALIAS.match_node(node) else {
            return;
        };
        let (Some(name), Some(original_name)) = (captures[0].sym(), captures[1].sym()) else {
            return;
        };
        if name == original_name {
            self.track_intentional_redefinition(node, name);
            return;
        }
        if inside_condition(node) {
            return;
        }
        self.found_instance_method(cx, node, name);
    }

    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(captures) = ALIAS_METHOD.match_node(node)
            && let (Some(name), Some(original_name)) = (captures[0].sym(), captures[1].sym())
        {
            if name == original_name {
                self.track_intentional_redefinition(node, name);
                return;
            }
            if inside_condition(node) {
                return;
            }
            self.found_instance_method(cx, node, name);
        } else if let Some((attr_name, args)) = node.attribute_accessor() {
            self.on_attr(cx, node, attr_name, &args);
        } else if let Some(names) = delegating_method(cx, node)
            .then(|| DELEGATE_ARGS.match_node(node))
            .flatten()
        {
            if inside_condition(node) {
                return;
            }
            let names: Vec<String> = names[0]
                .list()
                .iter()
                .filter_map(|c| sym_value(c.val()).map(str::to_owned))
                .collect();
            self.on_delegate(cx, node, &names);
        } else if let Some(captures) = DELEGATOR.match_node(node) {
            if inside_condition(node) {
                return;
            }
            if let Some(name) = sym_value(captures[0].val()) {
                self.found_instance_method(cx, node, name);
            }
        } else if let Some(captures) = DELEGATORS.match_node(node) {
            if inside_condition(node) {
                return;
            }
            let names: Vec<String> = captures[0]
                .list()
                .iter()
                .filter_map(|c| sym_value(c.val()).map(str::to_owned))
                .collect();
            for name in names {
                self.found_instance_method(cx, node, &name);
            }
        } else if let Some(captures) = ACTIVE_SUPPORT_REDEFINITION_MARKER.match_node(node)
            && let Some(name) = sym_value(captures[0].val())
            && active_support_extensions_enabled(cx)
        {
            let target = node.block_node().unwrap_or(node);
            self.track_intentional_redefinition(target, name);
        }
    }

    fn check_const_receiver<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        name: &str,
        const_name: &str,
    ) {
        let Some(qualified) = lookup_constant(node, const_name) else {
            return;
        };
        self.found_method(cx, node, &format!("{qualified}.{name}"), None);
    }

    fn check_self_receiver<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, name: &str) {
        if let Some(enclosing) = node.parent_module_name() {
            self.found_method(cx, node, &format!("{enclosing}.{name}"), None);
        } else if let Some(anon_block) = anonymous_class_block(node) {
            let scope = qualified_name(anon_block.parent_module_name().as_deref(), None, "Object");
            let scope_id = anon_block_scope_id(cx, anon_block);
            self.found_method(cx, node, &format!("{scope}.{name}"), scope_id);
        }
    }

    fn on_delegate<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, method_names: &[String]) {
        let name_prefix = delegate_prefix(node);
        for name in method_names {
            let name = match &name_prefix {
                Some(prefix) => format!("{prefix}_{name}"),
                None => name.clone(),
            };
            self.found_instance_method(cx, node, &name);
        }
    }

    fn found_instance_method<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, name: &str) {
        if let Some(scope) = node.parent_module_name() {
            self.found_method(cx, node, &format!("{}{name}", humanize_scope(&scope)), None);
        } else if let Some(anon_block) = anonymous_class_block(node) {
            let base = qualified_name(anon_block.parent_module_name().as_deref(), None, "Object");
            let scope = if node.ancestors().any(|a| a.is(NodeType::Sclass)) {
                format!("#<Class:{base}>")
            } else {
                base
            };
            let scope_id = anon_block_scope_id(cx, anon_block);
            self.found_method(
                cx,
                node,
                &format!("{}{name}", humanize_scope(&scope)),
                scope_id,
            );
        } else {
            self.found_sclass_method(cx, node, name);
        }
    }

    fn found_sclass_method<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, name: &str) {
        let Some(singleton_ancestor) = node.ancestors().find(|a| a.is(NodeType::Sclass)) else {
            return;
        };
        let Some(singleton_receiver_node) = singleton_ancestor.child(0) else {
            return;
        };
        if !singleton_receiver_node.is_send() {
            return;
        }
        let method = singleton_receiver_node.method_name().unwrap_or_default();
        self.found_method(cx, node, &format!("{method}.{name}"), None);
    }

    // `found_redefinition` needs `&mut self`, so the entry API does not fit.
    #[allow(clippy::map_entry)]
    fn found_method<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        method_name: &str,
        scope_id: Option<String>,
    ) {
        let mut key = method_key(node, method_name);
        if let Some(scope_id) = &scope_id {
            key = format!("{key}@{scope_id}");
        }
        let scope = node
            .ancestors()
            .find(|a| a.is_any(&[NodeType::Rescue, NodeType::Ensure]))
            .map(|a| a.ty());
        if self.definitions.contains_key(&key) {
            self.found_redefinition(cx, node, method_name, &key, scope);
        } else {
            self.definitions.insert(key, definition(cx, node));
            if scope_id.is_none() && scope.is_none() {
                self.check_cross_file_duplicate(cx, node, method_name);
            }
        }
    }

    /// `check_cross_file_duplicate`: with the project index, a method also
    /// defined in another file of the project.
    fn check_cross_file_duplicate<'a>(
        &self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        method_name: &str,
    ) {
        let Some(index) = cx.project_index() else {
            return;
        };
        if self.intentionally_redefined.contains(method_name)
            || node.ancestors().any(|a| a.is_any_def())
        {
            return;
        }
        let Some(prior) = cross_file_prior_definition(cx, &index, method_name) else {
            return;
        };
        let Some(defined) = index_source_location(&index, prior) else {
            return;
        };
        let message = format!(
            "Method `{method_name}` is defined at both {defined} and {}.",
            source_location(cx, node)
        );
        cx.add_offense(location(node), &message);
    }

    fn found_redefinition<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        method_name: &str,
        key: &str,
        scope: Option<NodeType>,
    ) {
        if let Some(scope) = scope
            && !self.scopes.get(&scope).is_some_and(|keys| keys.iter().any(|k| k == key))
        {
            self.definitions.insert(key.to_owned(), definition(cx, node));
            self.scopes.entry(scope).or_default().push(key.to_owned());
        } else if self.intentional_cross_file_redefinition(cx, method_name, key)
            || self.allowed_cross_file_redefinition(cx, key)
        {
            self.definitions.insert(key.to_owned(), definition(cx, node));
        } else {
            let defined = &self.definitions[key];
            let message = format!(
                "Method `{method_name}` is defined at both {}:{} and {}.",
                defined.path,
                defined.line,
                source_location(cx, node)
            );
            cx.add_offense(location(node), &message);
        }
    }

    fn allowed_cross_file_redefinition(&self, cx: &Investigation<'_, '_>, key: &str) -> bool {
        let other_path = &self.definitions[key].path;
        if *other_path == current_path(cx) {
            return false;
        }
        allowed_cross_file_path(cx, other_path)
    }

    fn intentional_cross_file_redefinition(
        &self,
        cx: &Investigation<'_, '_>,
        method_name: &str,
        key: &str,
    ) -> bool {
        self.intentionally_redefined.contains(method_name)
            && self.definitions[key].path != current_path(cx)
    }

    fn track_intentional_redefinition(&mut self, node: Node<'_>, name: &str) {
        let Some(scope) = node.parent_module_name() else {
            return;
        };
        self.intentionally_redefined
            .insert(format!("{}{name}", humanize_scope(&scope)));
    }

    fn on_attr<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        attr_name: &str,
        args: &[Node<'a>],
    ) {
        match attr_name {
            "attr" => {
                let writable = args.len() == 2 && args[1].is(NodeType::True);
                self.found_attr(cx, node, &args[..1.min(args.len())], true, writable);
            }
            "attr_reader" => self.found_attr(cx, node, args, true, false),
            "attr_writer" => self.found_attr(cx, node, args, false, true),
            "attr_accessor" => self.found_attr(cx, node, args, true, true),
            _ => {}
        }
    }

    fn found_attr<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        args: &[Node<'a>],
        readable: bool,
        writable: bool,
    ) {
        for arg in args {
            let Some(name) = SYM_NAME.match_node(*arg).and_then(|c| c[0].sym()) else {
                continue;
            };
            if readable {
                self.found_instance_method(cx, node, name);
            }
            if writable {
                self.found_instance_method(cx, node, &format!("{name}="));
            }
        }
    }
}

fn current_path(cx: &Investigation<'_, '_>) -> String {
    smart_path(cx.processed_source.path())
}

fn definition(cx: &Investigation<'_, '_>, node: Node<'_>) -> Definition {
    Definition {
        path: current_path(cx),
        line: node.first_line(),
    }
}

fn inside_condition(node: Node<'_>) -> bool {
    node.ancestors().any(|a| a.is(NodeType::If))
}

fn active_support_extensions_enabled(cx: &Investigation<'_, '_>) -> bool {
    cx.config.all_cops_bool("ActiveSupportExtensionsEnabled", false)
}

fn delegating_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !active_support_extensions_enabled(cx) {
        return false;
    }
    let name = node.method_name().unwrap_or_default();
    match cx.cop_config_value("DelegatingMethods") {
        None => name == "delegate",
        Some(_) => cx.cop_config_list("DelegatingMethods").contains(&name),
    }
}

fn delegate_prefix(node: Node<'_>) -> Option<String> {
    let kwargs_node = node.last_argument()?;
    let prefix = hash_value(kwargs_node, "prefix")?;
    if prefix.is(NodeType::True) {
        let to = hash_value(kwargs_node, "to")?;
        literal_value(to)
    } else if prefix.is_any(&[NodeType::Sym, NodeType::Str]) {
        literal_value(prefix)
    } else {
        None
    }
}

fn literal_value(node: Node<'_>) -> Option<String> {
    match node.value()? {
        Child::Sym(s) | Child::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// `node.pairs.find { |pair| pair.key.value == key }&.value`: only symbol
/// keys equal the symbol.
fn hash_value<'a>(node: Node<'a>, key: &str) -> Option<Node<'a>> {
    node.pairs()
        .into_iter()
        .find(|pair| {
            pair.key()
                .is_some_and(|k| k.is(NodeType::Sym) && k.sym_value() == Some(key))
        })
        .and_then(|pair| pair.pair_value())
}

fn humanize_scope(scope: &str) -> String {
    let scope = regex!(r"(?:(.*)::)#<Class:\1>|#<Class:(.*)>(?:::)?").replace(
        scope,
        |caps: &onig::Captures<'_>| {
            format!("{}.", caps.at(1).or_else(|| caps.at(2)).unwrap_or(""))
        },
    );
    if scope.ends_with('.') {
        scope
    } else {
        format!("{scope}#")
    }
}

fn anonymous_class_block(node: Node<'_>) -> Option<Node<'_>> {
    let first_block = node.ancestors().find(|a| a.is(NodeType::Block))?;
    if !CLASS_OR_MODULE_NEW_BLOCK.is_match(first_block) {
        return None;
    }
    if first_block.parent().is_some_and(|p| p.is(NodeType::Lvasgn)) {
        return None;
    }
    if node
        .ancestors()
        .filter(|a| a.is(NodeType::Sclass))
        .any(|s| !s.child(0).is_some_and(|c| c.is(NodeType::SelfNode)))
    {
        return None;
    }
    Some(first_block)
}

fn anon_block_scope_id(cx: &Investigation<'_, '_>, anon_block: Node<'_>) -> Option<String> {
    let parent = anon_block.parent()?;
    let allowed = parent.is_any_block()
        || parent.is_call()
        || parent.is_any_def()
        || parent.is_any(&[NodeType::Begin, NodeType::Casgn, NodeType::Ivasgn]);
    if !allowed {
        return None;
    }
    if let Some(receiver) = scope_receiver(parent, anon_block) {
        Some(format!(
            "{}.{}",
            receiver.source(),
            parent.method_name().unwrap_or_default()
        ))
    } else if !parent.is(NodeType::Begin) || parent.parent().is_some_and(|p| p.is_any_block()) {
        Some(anon_block_identity(cx, anon_block))
    } else {
        None
    }
}

fn scope_receiver<'a>(parent: Node<'a>, anon_block: Node<'a>) -> Option<Node<'a>> {
    if CLASS_NEW_BLOCK.is_match(anon_block) && parent.is_call() {
        return None;
    }
    named_receiver(parent)
}

fn named_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver()?;
    (!CLASS_OR_MODULE_NEW_BLOCK.is_match(receiver)).then_some(receiver)
}

fn method_key(node: Node<'_>, method_name: &str) -> String {
    match node.ancestors().find(|a| a.is_any_def()) {
        Some(ancestor_def) => format!(
            "{}.{method_name}",
            ancestor_def.method_name().unwrap_or_default()
        ),
        None => method_name.to_owned(),
    }
}

fn location(node: Node<'_>) -> Range {
    if node.is_any_def() {
        node.loc().keyword.unwrap().join(node.loc().name.unwrap())
    } else {
        node.source_range()
    }
}

/// `defined_module`: the namespace and name of the constant a `class`,
/// `module` or `Class.new`/`Module.new` assignment defines.
fn defined_module(node: Node<'_>) -> Option<(Option<Node<'_>>, &str)> {
    match node.ty() {
        NodeType::Class | NodeType::Module => {
            let identifier = node.child(0).filter(|c| c.is(NodeType::Const))?;
            Some((identifier.child(0), identifier.child_sym(1)?))
        }
        NodeType::Casgn => {
            node.defined_module_name()?;
            Some((node.child(0), node.child_sym(1)?))
        }
        _ => None,
    }
}

fn lookup_constant(node: Node<'_>, const_name: &str) -> Option<String> {
    for ancestor in node
        .ancestors()
        .filter(|a| a.is_any(&[NodeType::Class, NodeType::Module, NodeType::Casgn]))
    {
        let (mut namespace, mut mod_name) = match defined_module(ancestor) {
            Some((namespace, name)) => (namespace, Some(name)),
            None => (None, None),
        };
        loop {
            if mod_name == Some(const_name) {
                return Some(qualified_name(
                    ancestor.parent_module_name().as_deref(),
                    namespace,
                    const_name,
                ));
            }
            let Some(ns) = namespace else {
                break;
            };
            // `namespace, mod_name = *namespace`
            namespace = ns.child(0);
            mod_name = ns.child_sym(1);
        }
    }
    None
}

fn qualified_name(enclosing: Option<&str>, namespace: Option<Node<'_>>, mod_name: &str) -> String {
    let namespace_name = namespace.map(|ns| ns.const_name().unwrap_or_default());
    if enclosing != Some("Object") {
        let enclosing = enclosing.unwrap_or_default();
        match namespace_name {
            Some(ns) => format!("{enclosing}::{ns}::{mod_name}"),
            None => format!("{enclosing}::{mod_name}"),
        }
    } else if let Some(ns) = namespace_name {
        format!("{ns}::{mod_name}")
    } else {
        mod_name.to_owned()
    }
}

fn source_location(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    format!("{}:{}", current_path(cx), node.first_line())
}

fn allowed_cross_file_path(cx: &Investigation<'_, '_>, path: &str) -> bool {
    let patterns = cx.cop_config_list("AllowedCrossFilePaths");
    if patterns.is_empty() {
        return false;
    }
    let relative = cx
        .config
        .path_relative_to_config(std::path::Path::new(path));
    patterns
        .iter()
        .any(|pattern| match_path(pattern, &relative) || match_path(pattern, path))
}

fn anon_block_identity(cx: &Investigation<'_, '_>, anon_block: Node<'_>) -> String {
    let range = anon_block.source_range();
    format!(
        "{}:{}:{}",
        current_path(cx),
        anon_block.first_line(),
        range.begin_pos
    )
}

/// `cross_file_prior_definition`.
fn cross_file_prior_definition(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    method_name: &str,
) -> Option<Def> {
    let indexable = regex!(r"\A([A-Z]\w*(?:::[A-Z]\w*)*)([#.])([^#.]+)\z");
    let captures = indexable.captures(method_name)?;
    let (owner, separator, name) = (captures.at(1)?, captures.at(2)?, captures.at(3)?);
    let definitions: Vec<Def> =
        definitions_in_other_files(cx, index, &indexed_definitions(index, owner, separator, name))
            .into_iter()
            .filter(|definition| {
                !definition_path(index, *definition)
                    .is_some_and(|path| allowed_cross_file_path(cx, &path))
            })
            .collect();
    if definitions.is_empty()
        || cross_file_self_alias_trick(index, &definitions)
        || cross_file_redefinition_marker(cx, index, &definitions, name)
    {
        return None;
    }
    // `first_indexed_definition`
    definitions
        .into_iter()
        .find(|definition| definition.kind != DefKind::MethodAlias)
}

fn definition_path(index: &ProjectIndex, definition: Def) -> Option<String> {
    index
        .location(definition)?
        .file_path
        .map(|path| path.to_string_lossy().into_owned())
}

/// `indexed_definitions`: rubydex indexes `attr_writer :foo` under `foo`
/// rather than `foo=`, so writer definitions come from both the `foo=` and
/// the `foo` declarations.
fn indexed_definitions(index: &ProjectIndex, owner: &str, separator: &str, name: &str) -> Vec<Def> {
    let namespace = if separator == "." {
        format!("{owner}::<{}>", owner.rsplit("::").next().unwrap_or(owner))
    } else {
        owner.to_owned()
    };
    let declaration_definitions = |name: &str| {
        index
            .get(&format!("{namespace}#{name}()"))
            .map(|declaration| index.definitions(declaration))
            .unwrap_or_default()
    };
    if regex!(r"\A\w+=\z").find(name).is_some() {
        let mut definitions = declaration_definitions(name);
        definitions.extend(
            declaration_definitions(name.strip_suffix('=').unwrap_or(name))
                .into_iter()
                .filter(|definition| {
                    matches!(definition.kind, DefKind::AttrWriter | DefKind::AttrAccessor)
                }),
        );
        definitions
    } else {
        declaration_definitions(name)
            .into_iter()
            .filter(|definition| definition.kind != DefKind::AttrWriter)
            .collect()
    }
}

/// `cross_file_self_alias_trick?`: an alias of the method alongside one of
/// its definitions in another file may be the self-alias trick marking an
/// intentional redefinition there.
fn cross_file_self_alias_trick(index: &ProjectIndex, definitions: &[Def]) -> bool {
    let (aliases, others): (Vec<Def>, Vec<Def>) = definitions
        .iter()
        .partition(|definition| definition.kind == DefKind::MethodAlias);
    let alias_paths: Vec<Option<String>> = aliases
        .iter()
        .map(|definition| definition_path(index, *definition))
        .collect();
    others
        .iter()
        .any(|definition| alias_paths.contains(&definition_path(index, *definition)))
}

/// `cross_file_redefinition_marker?`: an Active Support redefinition marker
/// naming the method in the other file's source.
fn cross_file_redefinition_marker(
    cx: &Investigation<'_, '_>,
    index: &ProjectIndex,
    definitions: &[Def],
    name: &str,
) -> bool {
    if !active_support_extensions_enabled(cx) {
        return false;
    }
    let mut paths: Vec<String> = Vec::new();
    for path in definitions
        .iter()
        .filter_map(|definition| definition_path(index, *definition))
    {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
        .iter()
        .any(|path| redefinition_marker_in_file(path, name))
}

/// `redefinition_marker_in_file?`.
fn redefinition_marker_in_file(path: &str, name: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let escaped = crate::cop::mixin::util::regexp_escape(name);
    let Ok(marker) = onig::Regex::new(&format!(
        r#"\b(?:silence_redefinition_of_method|redefine_method)\s*\(?\s*(?::{escaped}|(["']){escaped}\1)(?![\w=!?])"#
    )) else {
        return false;
    };
    marker.find(&String::from_utf8_lossy(&bytes)).is_some()
}

/// `index_source_location`.
fn index_source_location(index: &ProjectIndex, definition: Def) -> Option<String> {
    let location = index.location(definition)?;
    Some(format!(
        "{}:{}",
        smart_path(location.file_path.as_deref()?),
        location.start_line + 1
    ))
}
