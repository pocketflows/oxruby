//! Port of `lib/rubocop/cop/naming/file_name.rb`.

use std::path::Path;

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::{config_regexp, regexp_to_s};
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct FileName;

impl Cop for FileName {
    fn name(&self) -> &'static str {
        "Naming/FileName"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let file_path = cx.processed_source.file_path();
        let path = Path::new(&file_path);
        if cx.config.file_to_exclude(path) || cx.config.allowed_camel_case_file(path) {
            return;
        }
        for_bad_filename(cx, &file_path);
    }
}

/// The `Regex` parameter: a Ruby `Regexp` (`!ruby/regexp /.../i` in YAML)
/// or a string, with how `format` shows it.
struct ConfigRegex {
    regex: onig::Regex,
    display: String,
}

fn regex_param(cx: &Investigation<'_, '_>) -> Option<ConfigRegex> {
    let value = cx.cop_config_value("Regex")?;
    let display = match value {
        Value::String(source) => source.clone(),
        other => regexp_to_s(other)?,
    };
    Some(ConfigRegex {
        regex: config_regexp(value)?,
        display,
    })
}

fn for_bad_filename(cx: &mut Investigation<'_, '_>, file_path: &str) {
    let basename = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let regex = regex_param(cx);
    let msg = if filename_good(&basename, regex.as_ref()) {
        perform_class_and_module_naming_checks(cx, file_path, &basename)
    } else if bad_filename_allowed(cx) {
        None
    } else {
        Some(match &regex {
            Some(regex) => format!("`{basename}` should match `{}`.", regex.display),
            None => {
                format!("The name of this source file (`{basename}`) should use snake_case.")
            }
        })
    };
    if let Some(msg) = msg {
        cx.add_global_offense(&msg);
    }
}

fn perform_class_and_module_naming_checks(
    cx: &Investigation<'_, '_>,
    file_path: &str,
    basename: &str,
) -> Option<String> {
    if !cx.cop_config_bool("ExpectMatchingDefinition", false) {
        return None;
    }
    if cx.cop_config_bool("CheckDefinitionPathHierarchy", false)
        && !find_class_or_module(cx, to_namespace(cx, file_path))
    {
        Some(no_definition_message(cx, basename, file_path))
    } else if !find_class_or_module(cx, to_namespace(cx, basename)) {
        Some(no_definition_message(cx, basename, basename))
    } else {
        None
    }
}

fn bad_filename_allowed(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreExecutableScripts", false) && cx.processed_source.starts_with("#!")
}

fn no_definition_message(cx: &Investigation<'_, '_>, basename: &str, file_path: &str) -> String {
    let namespace = to_namespace(cx, file_path).join("::");
    format!("`{basename}` should define a class or module called `{namespace}`.")
}

fn filename_good(basename: &str, regex: Option<&ConfigRegex>) -> bool {
    let basename = basename.strip_prefix('.').unwrap_or(basename);
    let basename = regex!(r"\.[^.]+$").replace(basename, "");
    // special handling for Action Pack Variants file names like
    // some_file.xlsx+mobile.axlsx
    let basename = basename.replacen('+', "_", 1);
    match regex {
        Some(regex) => regex.regex.find(&basename).is_some(),
        None => regex!(r"^[\d[[:lower:]]_.?!]+$").find(&basename).is_some(),
    }
}

/// A constant as `defined_module` returns it: `(const namespace name)`.
type Const<'a> = (Option<Node<'a>>, &'a str);

fn find_class_or_module(cx: &Investigation<'_, '_>, mut namespace: Vec<String>) -> bool {
    let Some(root) = cx.processed_source.ast() else {
        return false;
    };
    let name = namespace.pop().unwrap_or_default();
    for child in root
        .each_node()
        .filter(|n| matches!(n.ty(), NodeType::Class | NodeType::Module | NodeType::Casgn))
    {
        let Some((const_namespace, const_name)) = find_definition(child) else {
            continue;
        };
        if name != const_name && !match_acronym(cx, &name, const_name) {
            continue;
        }
        if !(namespace.is_empty()
            || namespace_matches(cx, child, const_namespace, &mut namespace))
        {
            continue;
        }
        return true;
    }
    false
}

fn find_definition(node: Node<'_>) -> Option<Const<'_>> {
    defined_module(node).or_else(|| defined_struct(node))
}

/// rubocop-ast's `defined_module`.
fn defined_module(node: Node<'_>) -> Option<Const<'_>> {
    match node.ty() {
        NodeType::Class | NodeType::Module => {
            let identifier = node.child(0).filter(|c| c.is(NodeType::Const))?;
            Some((identifier.child(0), identifier.child_sym(1)?))
        }
        NodeType::Casgn => {
            let value = casgn_value(node)?;
            let send = if value.is(NodeType::Block) {
                value.child(0)?
            } else {
                value
            };
            let constructor = send.is(NodeType::Send)
                && send.is_method("new")
                && send
                    .receiver()
                    .is_some_and(|r| r.is_global_const("Class") || r.is_global_const("Module"));
            constructor.then_some((node.child(0), node.child_sym(1)?))
        }
        _ => None,
    }
}

fn casgn_value(node: Node<'_>) -> Option<Node<'_>> {
    node.child(2)
}

/// `struct_definition`.
fn defined_struct(node: Node<'_>) -> Option<Const<'_>> {
    if !node.is(NodeType::Casgn) {
        return None;
    }
    let value = casgn_value(node)?;
    let send = if value.is(NodeType::Block) {
        value.child(0)?
    } else {
        value
    };
    let is_struct = send.is(NodeType::Send)
        && send.is_method("new")
        && send.receiver().is_some_and(|r| r.is_global_const("Struct"));
    is_struct.then_some((node.child(0), node.child_sym(1)?))
}

fn namespace_matches<'a>(
    cx: &Investigation<'_, '_>,
    node: Node<'a>,
    namespace: Option<Node<'a>>,
    expected: &mut Vec<String>,
) -> bool {
    partial_match(cx, namespace, expected);
    for ancestor in node.ancestors().filter(|a| {
        matches!(
            a.ty(),
            NodeType::Class | NodeType::Module | NodeType::Sclass | NodeType::Casgn
        )
    }) {
        if ancestor.is(NodeType::Sclass) {
            return false;
        }
        if let Some((parent, name)) = defined_module(ancestor) {
            pop_matching(cx, expected, name);
            partial_match(cx, parent, expected);
        }
    }
    matches(expected)
}

/// The `partial_matcher!` lambda: walks a constant's namespace chain,
/// popping the expected names it finds.
fn partial_match(cx: &Investigation<'_, '_>, mut namespace: Option<Node<'_>>, expected: &mut Vec<String>) {
    while let Some(node) = namespace {
        if node.is(NodeType::Cbase) {
            return;
        }
        namespace = node.child(0);
        if let Some(name) = node.child_sym(1) {
            pop_matching(cx, expected, name);
        }
    }
}

fn pop_matching(cx: &Investigation<'_, '_>, expected: &mut Vec<String>, name: &str) {
    if let Some(last) = expected.last()
        && (last == name || match_acronym(cx, last, name))
    {
        expected.pop();
    }
}

fn matches(expected: &[String]) -> bool {
    expected.is_empty() || expected == ["Object"]
}

fn match_acronym(cx: &Investigation<'_, '_>, expected: &str, name: &str) -> bool {
    let name = cx
        .cop_config_list("AllowedAcronyms")
        .iter()
        .fold(name.to_owned(), |result, acronym| {
            result.replace(acronym, &capitalize(acronym))
        });
    expected == name
}

fn to_namespace(cx: &Investigation<'_, '_>, path: &str) -> Vec<String> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    // To convert a pathname to a Ruby namespace, we need a starting point.
    // To find the closest namespace root take the path components, and then
    // work through them backwards until we find a candidate.
    let start = cx.cop_config_list("CheckDefinitionPathHierarchyRoots");
    let start_index = components
        .iter()
        .rposition(|c| start.contains(c))
        .map(|i| i + 1);
    match start_index {
        None => vec![to_module_name(components.last().copied().unwrap_or(""))],
        Some(start_index) => components[start_index..]
            .iter()
            .map(|c| to_module_name(c))
            .collect(),
    }
}

fn to_module_name(basename: &str) -> String {
    let basename = basename.split_once('.').map_or(basename, |(stem, _)| stem);
    let mut words: Vec<&str> = basename.split('_').collect();
    // `String#split` drops trailing empty fields.
    while words.last().is_some_and(|w| w.is_empty()) {
        words.pop();
    }
    words.iter().map(|w| capitalize(w)).collect()
}

/// Ruby's `String#capitalize`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars.as_str().to_lowercase().chars()).collect(),
        None => String::new(),
    }
}

