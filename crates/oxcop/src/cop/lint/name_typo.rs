//! Port of `lib/rubocop/cop/lint/name_typo.rb`.

use std::collections::HashSet;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::project_index_help::{
    fully_resolved_index_ancestry, indexed_singleton_member, indexed_singleton_of,
    resolve_constant_in_index,
};
use crate::cop::{Cop, Investigation};
use crate::project_index::{Decl, ProjectIndex};

#[derive(Default)]
pub struct NameTypo {
    literal_names: Option<HashSet<String>>,
    bundled_gem_namespaces: Option<HashSet<String>>,
}

impl Cop for NameTypo {
    fn name(&self) -> &'static str {
        "Lint/NameTypo"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const, NodeType::Send, NodeType::Csend]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.literal_names = None;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Const) {
            self.on_const(cx, node);
        } else {
            self.on_send(cx, node);
        }
    }
}

impl NameTypo {
    fn on_const<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = check(cx, "CheckConstants") else {
            return;
        };
        if !checkable_constant(cx, node) {
            return;
        }
        let Some(suggestion) = self.constant_typo_suggestion(cx, &index, node) else {
            return;
        };
        let (Some(name), Some(namespace), Some(range)) = (
            node.short_name(),
            node.namespace().and_then(|ns| ns.const_name()),
            node.loc().name,
        ) else {
            return;
        };
        let message = format!(
            "Possible typo: `{name}` is not defined in `{namespace}`. Did you mean `{suggestion}`?"
        );
        cx.add_offense(range, &message);
    }

    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index) = check(cx, "CheckMethods") else {
            return;
        };
        let Some(receiver) = node.receiver().filter(|r| r.is(NodeType::Const)) else {
            return;
        };
        let Some(method_name) = node.method_name() else {
            return;
        };
        if allowed_name(cx, method_name) || defined_check(node) {
            return;
        }
        let Some(suggestion) = self.method_typo_suggestion(cx, &index, node) else {
            return;
        };
        let Some(selector) = node.loc().selector else {
            return;
        };
        let message = format!(
            "Possible typo: `{}` does not respond to `{method_name}`. Did you mean `{suggestion}`?",
            receiver.const_name().unwrap_or_default()
        );
        cx.add_offense(selector, &message);
    }

    fn constant_typo_suggestion(
        &mut self,
        cx: &Investigation<'_, '_>,
        index: &ProjectIndex,
        node: Node<'_>,
    ) -> Option<String> {
        let namespace = resolve_constant_in_index(index, node.namespace()?)?;
        if !namespace.is_namespace() {
            return None;
        }
        let short_name = node.short_name()?;
        if index.find_member(namespace, short_name).is_some()
            || !self.complete_index_members(cx, index, namespace)
            || self.literal_names(cx).contains(short_name)
        {
            return None;
        }
        spell_check(short_name, constant_member_names(index, namespace))
    }

    /// `complete_index_members?`: whether the index can be trusted to list
    /// everything the namespace defines.
    fn complete_index_members(
        &mut self,
        cx: &Investigation<'_, '_>,
        index: &ProjectIndex,
        declaration: Decl,
    ) -> bool {
        fully_resolved_index_ancestry(index, declaration, false)
            && !self.gem_owned_namespace(cx, index, declaration)
    }

    /// `gem_owned_namespace?`: whether the namespace's root segment names a
    /// gem in the bundle, which is then free to define members the index
    /// never sees.
    fn gem_owned_namespace(
        &mut self,
        cx: &Investigation<'_, '_>,
        index: &ProjectIndex,
        declaration: Decl,
    ) -> bool {
        if cx
            .config
            .for_all_cops()
            .get("ProjectIndexIncludesGems")
            .is_some_and(truthy)
        {
            return false;
        }
        let name = index.name(declaration);
        let root = name.split("::").next().unwrap_or_default().to_lowercase();
        self.bundled_gem_namespaces(cx).contains(&root)
    }

    /// `bundled_gem_namespaces`: gem names normalized towards the constant
    /// they conventionally provide, from the bundle's gems except those
    /// sourced from a local path (`bundled_gem_names`).
    fn bundled_gem_namespaces(&mut self, cx: &Investigation<'_, '_>) -> &HashSet<String> {
        self.bundled_gem_namespaces.get_or_insert_with(|| {
            let Some(gems) = crate::lockfile::target_gems(cx.config) else {
                return HashSet::new();
            };
            gems.names
                .iter()
                .filter(|name| !gems.path_sourced.contains(name))
                .map(|name| name.replace(['-', '_'], "").to_lowercase())
                .collect()
        })
    }

    /// `method_typo_suggestion`: writers are indexed under the reader's
    /// name, so setter calls are verified and spell-checked through the base
    /// name.
    fn method_typo_suggestion(
        &mut self,
        cx: &Investigation<'_, '_>,
        index: &ProjectIndex,
        node: Node<'_>,
    ) -> Option<String> {
        let method_name = node.method_name()?;
        let setter = setter_call(node);
        let base = if setter {
            method_name.strip_suffix('=').unwrap_or(method_name)
        } else {
            method_name
        };
        let declaration = self.unknown_method_owner(cx, index, node, method_name, base)?;
        if self.literal_names(cx).contains(base) {
            return None;
        }
        let suggestion = spell_check(base, method_member_names(index, declaration))?;
        Some(if setter {
            format!("{suggestion}=")
        } else {
            suggestion
        })
    }

    /// `unknown_method_owner`: the receiver's declaration when it resolves
    /// in the index, everything it can inherit or be reopened with is
    /// visible there, and the method is not found.
    fn unknown_method_owner(
        &mut self,
        cx: &Investigation<'_, '_>,
        index: &ProjectIndex,
        node: Node<'_>,
        name: &str,
        base: &str,
    ) -> Option<Decl> {
        let declaration = resolve_constant_in_index(index, node.receiver()?)?;
        if !declaration.is_namespace()
            || responds_in_index(index, declaration, name, base)
            || !fully_resolved_index_ancestry(index, declaration, false)
            || self.gem_owned_namespace(cx, index, declaration)
        {
            return None;
        }
        Some(declaration)
    }

    /// `literal_names`: names mentioned as symbols or inside string
    /// literals in the current file.
    fn literal_names(&mut self, cx: &Investigation<'_, '_>) -> &HashSet<String> {
        self.literal_names.get_or_insert_with(|| {
            let mut names = HashSet::new();
            let Some(root) = cx.processed_source.ast() else {
                return names;
            };
            let identifier = crate::regex!(r"[a-zA-Z_]\w*[?!]?");
            for literal in root.descendants() {
                if let Some(value) = literal.sym_value() {
                    names.insert(value.to_owned());
                } else if literal.is(NodeType::Str) {
                    // `scan` raises on binary string literals with invalid
                    // byte sequences, so they're scrubbed first.
                    let value = match literal.value() {
                        Some(Child::Str(s)) => s.clone(),
                        Some(Child::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                        _ => continue,
                    };
                    for found in identifier.find_iter(&value) {
                        names.insert(value[found.0..found.1].to_owned());
                    }
                }
            }
            names
        })
    }
}

/// `check?(key)`: the index, when the check is enabled.
fn check(cx: &Investigation<'_, '_>, key: &str) -> Option<std::sync::Arc<ProjectIndex>> {
    let index = cx.project_index()?;
    cx.cop_config_bool(key, true).then_some(index)
}

fn truthy(value: &serde_yaml_ng::Value) -> bool {
    !matches!(value, serde_yaml_ng::Value::Null | serde_yaml_ng::Value::Bool(false))
}

/// `checkable_constant?`.
fn checkable_constant(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.namespace().is_some_and(|ns| ns.is(NodeType::Const))
        && !node.short_name().is_some_and(|name| allowed_name(cx, name))
        && !definition_identifier(node)
        && !defined_check(node)
}

/// `allowed_name?`.
fn allowed_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    cx.cop_config_list("AllowedNames").contains(&name)
}

/// `definition_identifier?`: the last segment of a class, module or
/// constant definition is being defined, not referenced.
fn definition_identifier(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| parent.defined_module_name().is_some())
}

/// `defined_check?`: `defined?(Foo::Bar)` probes whether a name exists.
fn defined_check(node: Node<'_>) -> bool {
    node.ancestors().any(|a| a.is(NodeType::Defined))
}

/// `setter_call?`.
fn setter_call(node: Node<'_>) -> bool {
    node.is_assignment_method() && !node.is_operator_method()
}

/// `responds_in_index?`: instance methods of a module are also callable on
/// the module itself when exposed with `module_function`.
fn responds_in_index(index: &ProjectIndex, declaration: Decl, name: &str, base: &str) -> bool {
    let candidates: &[&str] = if name == base { &[name] } else { &[name, base] };
    candidates.iter().any(|candidate| {
        let member_name = format!("{candidate}()");
        indexed_singleton_member(index, declaration, &member_name).is_some()
            || index.find_member(declaration, &member_name).is_some()
    })
}

/// `constant_member_names`.
fn constant_member_names(index: &ProjectIndex, namespace: Decl) -> Vec<String> {
    index
        .members(namespace)
        .into_iter()
        .filter_map(|member| {
            let name = index.name(member);
            (!name.contains('#') && !name.contains('<'))
                .then(|| name.rsplit("::").next().unwrap_or_default().to_owned())
        })
        .collect()
}

/// `method_member_names`: the singleton-side methods of the declaration's
/// ancestry, and the declaration's own methods.
fn method_member_names(index: &ProjectIndex, declaration: Decl) -> Vec<String> {
    let method_member = crate::regex!(r"#([a-zA-Z_]\w*[?!]?)\(\)\z");
    let mut scopes: Vec<Decl> = index
        .ancestors(declaration)
        .into_iter()
        .filter_map(|ancestor| indexed_singleton_of(index, ancestor))
        .collect();
    scopes.push(declaration);
    scopes
        .into_iter()
        .flat_map(|scope| index.members(scope))
        .filter_map(|member| {
            let name = index.name(member);
            let captures = method_member.captures(&name)?;
            captures.at(1).map(str::to_owned)
        })
        .collect()
}

/// `spell_check`: `DidYouMean::SpellChecker.new(dictionary: dictionary.uniq)
/// .correct(name).first`.
fn spell_check(name: &str, dictionary: Vec<String>) -> Option<String> {
    let mut unique: Vec<&str> = Vec::new();
    for word in &dictionary {
        if !unique.contains(&word.as_str()) {
            unique.push(word);
        }
    }
    if unique.is_empty() {
        return None;
    }
    crate::name_similarity::spell_checker_correct(&unique, name)
        .into_iter()
        .next()
}
