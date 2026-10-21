//! Port of `lib/rubocop/cop/migration/department_name.rb`.

use std::sync::LazyLock;

use serde_yaml_ng::{Mapping, Value};

use crate::config::core_cops;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::directive_comment::CopRegistry;
use crate::regex;

const MSG: &str = "Department name is missing.";

#[derive(Default)]
pub struct DepartmentName;

impl Cop for DepartmentName {
    fn name(&self) -> &'static str {
        "Migration/DepartmentName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        for comment in ps.comments() {
            // DISABLE_COMMENT_FORMAT
            let Some(captures) =
                regex!(r"\A(# *rubocop *: *((dis|en)able|todo) +)(.*)").captures(comment.text)
            else {
                continue;
            };
            let mut offset = captures.at(1).unwrap_or("").len();
            let names = captures.at(4).unwrap_or("");
            for (begin, end) in regex!(r"[^,]+|\W+").find_iter(names) {
                let name = &names[begin..end];
                let trimmed_name = name.trim();
                if !valid_content_token(trimmed_name) {
                    check_cop_name(cx, trimmed_name, comment.range.begin_pos, offset);
                }
                if contain_unexpected_character_for_department_name(name) {
                    break;
                }
                offset += name.len();
            }
        }
    }
}

fn check_cop_name(cx: &mut Investigation<'_, '_>, name: &str, comment_begin: usize, offset: usize) {
    let start = comment_begin + offset;
    let range = range_between(start, start + name.len());
    let cop_name = cx.source(range);
    cx.add_offense_with_correction(range, MSG, |corrector| {
        let mut qualified_cop_name = qualified_cop_name(cop_name);
        if !qualified_cop_name.contains('/') {
            qualified_cop_name = qualified_legacy_cop_name(cop_name).unwrap_or_default();
        }
        corrector.replace(range, qualified_cop_name);
    });
}

/// `Registry.global.qualified_cop_name(cop_name, nil, warn: false)`.
fn qualified_cop_name(name: &str) -> String {
    let registry = core_cops();
    if name.contains('/') || registry.contains(name) {
        return name.to_owned();
    }
    let suffix = format!("/{name}");
    let candidates: Vec<&String> = registry
        .names()
        .iter()
        .filter(|n| n.ends_with(&suffix) && !n[..n.len() - suffix.len()].contains('/'))
        .collect();
    match candidates.as_slice() {
        [one] => (*one).clone(),
        // No namespace found (or an ambiguous one, which upstream raises).
        _ => name.to_owned(),
    }
}

fn valid_content_token(content_token: &str) -> bool {
    regex!(r"\W+").find(content_token).is_some()
        // DISABLING_COPS_CONTENT_TOKEN
        || regex!(r"[A-Za-z]+/[A-Za-z]+|all").find(content_token).is_some()
        || core_cops().is_department(content_token)
}

fn contain_unexpected_character_for_department_name(name: &str) -> bool {
    regex!(r"[^A-Za-z/, ]").find(name).is_some()
}

/// `qualified_legacy_cop_name(cop_name)`.
fn qualified_legacy_cop_name(cop_name: &str) -> Option<String> {
    legacy_cop_names()
        .iter()
        .find(|legacy| legacy.split('/').nth(1) == Some(cop_name))
        .cloned()
}

/// `ConfigObsoletion.legacy_cop_names`: the old names of the cop rules in
/// `config/obsoletion.yml` (renamed, removed, split, extracted, in that
/// order), except those disabled with `~`.
fn legacy_cop_names() -> &'static [String] {
    static NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let rules: Mapping = serde_yaml_ng::from_str(include_str!(
            "../../../../../vendor/rubocop/config/obsoletion.yml"
        ))
        .unwrap_or_default();
        ["renamed", "removed", "split", "extracted"]
            .iter()
            .filter_map(|rule_type| rules.get(*rule_type).and_then(Value::as_mapping))
            .flat_map(|data| {
                data.iter()
                    .filter(|(_, configuration)| !configuration.is_null())
                    .filter_map(|(cop_name, _)| cop_name.as_str().map(str::to_owned))
            })
            .collect()
    });
    &NAMES
}
