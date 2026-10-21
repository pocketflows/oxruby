//! Port of `lib/rubocop/cop/style/special_global_vars.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::{correct_style_detected, style, style_detected};
use crate::cop::mixin::require_library::{self, RequireLibrary};
use crate::cop::{Cop, Investigation};

const LIBRARY_NAME: &str = "English";

/// `ENGLISH_VARS` before the merges: each Perl name and its English names.
const ENGLISH: &[(&str, &[&str])] = &[
    ("$:", &["$LOAD_PATH"]),
    ("$\"", &["$LOADED_FEATURES"]),
    ("$0", &["$PROGRAM_NAME"]),
    ("$!", &["$ERROR_INFO"]),
    ("$@", &["$ERROR_POSITION"]),
    ("$;", &["$FIELD_SEPARATOR", "$FS"]),
    ("$,", &["$OUTPUT_FIELD_SEPARATOR", "$OFS"]),
    ("$/", &["$INPUT_RECORD_SEPARATOR", "$RS"]),
    ("$\\", &["$OUTPUT_RECORD_SEPARATOR", "$ORS"]),
    ("$.", &["$INPUT_LINE_NUMBER", "$NR"]),
    ("$_", &["$LAST_READ_LINE"]),
    ("$>", &["$DEFAULT_OUTPUT"]),
    ("$<", &["$DEFAULT_INPUT"]),
    ("$$", &["$PROCESS_ID", "$PID"]),
    ("$?", &["$CHILD_STATUS"]),
    ("$~", &["$LAST_MATCH_INFO"]),
    ("$=", &["$IGNORECASE"]),
    ("$*", &["$ARGV", "ARGV"]),
];

const NON_ENGLISH_VARS: &[&str] = &["$LOAD_PATH", "$LOADED_FEATURES", "$PROGRAM_NAME", "ARGV"];

/// `ENGLISH_VARS[global]`: English names (and themselves) for Perl names.
fn english_vars(global: &str) -> Option<Vec<&'static str>> {
    for (perl, english) in ENGLISH {
        if *perl == global {
            return Some(english.to_vec());
        }
        if let Some(name) = english.iter().find(|e| **e == global) {
            return Some(vec![name]);
        }
    }
    None
}

/// `PERL_VARS[global]`: the Perl name for English names (and themselves).
fn perl_vars(global: &str) -> Option<Vec<&'static str>> {
    for (perl, english) in ENGLISH {
        if english.contains(&global) || *perl == global {
            return Some(vec![perl]);
        }
    }
    None
}

/// `BUILTIN_VARS[global]`: like `PERL_VARS`, except that names built into
/// Ruby (`$LOAD_PATH`, `$LOADED_FEATURES`, `$PROGRAM_NAME`) are preferred.
fn builtin_vars(global: &str) -> Option<Vec<&'static str>> {
    for builtin in NON_ENGLISH_VARS.iter().filter(|v| v.starts_with('$')) {
        let perl = perl_vars(builtin).unwrap();
        if *builtin == global || perl.contains(&global) {
            return Some(vec![builtin]);
        }
    }
    perl_vars(global)
}

fn vars_for(style: &str, global: &str) -> Option<Vec<&'static str>> {
    match style {
        "use_english_names" => english_vars(global),
        "use_perl_names" => perl_vars(global),
        "use_builtin_english_names" => builtin_vars(global),
        _ => panic!("Invalid style: :{style}"),
    }
}

#[derive(Default)]
pub struct SpecialGlobalVars {
    require_library: RequireLibrary,
    required_english: bool,
}

impl Cop for SpecialGlobalVars {
    fn name(&self) -> &'static str {
        "Style/SpecialGlobalVars"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Gvar, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        require_library::RESTRICT_ON_SEND
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.require_library.reset();
        self.required_english = false;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_send() {
            self.require_library.on_send(node);
            return;
        }
        let global_var = node.name().unwrap_or_default();
        let style = style(cx);
        let Some(preferred) = vars_for(style, global_var) else {
            return;
        };
        if preferred.contains(&global_var) {
            correct_style_detected(cx, "EnforcedStyle");
            return;
        }
        style_detected(cx, "EnforcedStyle", &matching_styles(global_var));
        let message = message(style, global_var, &preferred);
        let should_require = style == "use_english_names"
            && cx.cop_config_bool("RequireEnglish", false)
            && !self.required_english
            && !NON_ENGLISH_VARS.contains(&preferred[0]);
        let ps = cx.processed_source;
        let require_library = &self.require_library;
        let mut required = false;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let mut node = node;
            while let Some(parent) = node
                .parent()
                .filter(|p| p.is(NodeType::Begin) && p.children().len() == 1)
            {
                node = parent;
            }
            if should_require {
                require_library.ensure_required(ps, corrector, node, LIBRARY_NAME);
                required = true;
            }
            corrector.replace(node.source_range(), replacement(style, node, preferred[0]));
        });
        if required {
            self.required_english = true;
        }
    }
}

fn message(style: &str, global_var: &str, preferred: &[&str]) -> String {
    if style == "use_english_names" {
        let (regular, english): (Vec<&str>, Vec<&str>) = english_vars(global_var)
            .unwrap_or_default()
            .into_iter()
            .partition(|var| NON_ENGLISH_VARS.contains(var));
        format_message(&english, &regular, global_var)
    } else {
        format!("Prefer `{}` over `{global_var}`.", preferred[0])
    }
}

fn format_message(english: &[&str], regular: &[&str], global: &str) -> String {
    if regular.is_empty() {
        format!(
            "Prefer `{}` from the stdlib 'English' module (don't forget to require it) over `{global}`.",
            english.join("` or `")
        )
    } else if english.is_empty() {
        format!("Prefer `{}` over `{global}`.", regular.join("` or `"))
    } else {
        format!(
            "Prefer `{}` from the stdlib 'English' module (don't forget to require it) or `{}` over `{global}`.",
            english.join("` or `"),
            regular.join("` or `")
        )
    }
}

fn replacement(style: &str, node: Node<'_>, preferred_name: &str) -> String {
    let in_string = node
        .parent()
        .is_some_and(|p| p.is_any(&[NodeType::Dstr, NodeType::Xstr, NodeType::Regexp]));
    if !in_string {
        return preferred_name.to_owned();
    }
    if style == "use_english_names" {
        if node.is(NodeType::Begin) {
            return format!("#{{{preferred_name}}}");
        }
        return format!("{{{preferred_name}}}");
    }
    format!("#{preferred_name}")
}

/// `matching_styles(global)`: the styles whose names include `global`.
fn matching_styles(global: &str) -> Vec<&'static str> {
    let english = ENGLISH.iter().any(|(_, names)| names.contains(&global));
    let perl = ENGLISH.iter().any(|(perl, _)| *perl == global);
    let builtin = (perl && !matches!(global, "$:" | "$\"" | "$0"))
        || matches!(global, "$LOAD_PATH" | "$LOADED_FEATURES" | "$PROGRAM_NAME");
    [
        ("use_english_names", english),
        ("use_perl_names", perl),
        ("use_builtin_english_names", builtin),
    ]
    .into_iter()
    .filter_map(|(style, matches)| matches.then_some(style))
    .collect()
}
