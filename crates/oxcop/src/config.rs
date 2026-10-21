//! Port of `lib/rubocop/config.rb`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

use serde_yaml_ng::{Mapping, Value};

use crate::path_util::{self, match_path};

static EMPTY: LazyLock<Mapping> = LazyLock::new(Mapping::new);

/// The file name RuboCop gives to source that did not come from a file.
pub const STRING_SOURCE_NAME: &str = "(string)";

#[derive(Debug, Clone)]
pub struct Config {
    hash: Mapping,
    loaded_path: Option<PathBuf>,
    base_dir_for_path_parameters: PathBuf,
    known_cops: OnceLock<KnownCops>,
    badge_configs: OnceLock<std::collections::HashMap<String, Mapping>>,
    /// `--enable-pending-cops` / `--disable-pending-cops` (the registry's
    /// options upstream).
    pub pending_cops_option: Option<bool>,
    /// `--preview` / `--no-preview` (`Config#preview?(options)`).
    pub preview_option: Option<bool>,
    /// `Config#target_ruby` (`TargetRuby.new(self)`), resolved once.
    target_ruby: OnceLock<crate::target_ruby::Source>,
}

/// The cops RuboCop's registry would know about for this configuration:
/// every cop in the merged configuration (core cops from `default.yml`,
/// plus plugin and custom cops the configuration mentions).
#[derive(Debug, Clone, Default)]
pub struct KnownCops {
    names: Vec<String>,
    departments: HashSet<String>,
}

impl KnownCops {
    fn from_config(hash: &Mapping) -> Self {
        let mut names: Vec<String> = hash
            .keys()
            .filter_map(Value::as_str)
            .filter(|key| key.contains('/'))
            .map(str::to_owned)
            .collect();
        names.sort();
        names.dedup();
        let mut departments = HashSet::new();
        for name in &names {
            let mut department = name.as_str();
            while let Some((parent, _)) = department.rsplit_once('/') {
                departments.insert(parent.to_owned());
                department = parent;
            }
        }
        Self { names, departments }
    }
}

/// `Cop::Registry.global`: every core cop (default.yml's), whether ported
/// or not.
pub fn core_cops() -> &'static KnownCops {
    static CORE: LazyLock<KnownCops> =
        LazyLock::new(|| KnownCops::from_config(crate::config_loader::default_configuration()));
    &CORE
}

impl KnownCops {
    /// `Registry#names.include?` / `find_by_cop_name`.
    pub fn contains(&self, cop_name: &str) -> bool {
        self.names
            .binary_search_by(|name| name.as_str().cmp(cop_name))
            .is_ok()
    }
}

impl crate::directive_comment::CopRegistry for KnownCops {
    fn names(&self) -> &[String] {
        &self.names
    }

    fn is_department(&self, name: &str) -> bool {
        self.departments.contains(name)
    }

    fn names_for_department(&self, department: &str) -> Vec<String> {
        self.names
            .iter()
            .filter(|name| {
                name.rsplit_once('/')
                    .is_some_and(|(dept, _)| dept == department)
            })
            .cloned()
            .collect()
    }
}

/// The registry as a run with `--only` sees it: cops named there count as
/// enabled (`Registry#enabled_cop_name?`), so the config doesn't disable them.
pub struct SelectedCops<'a> {
    pub known: &'a KnownCops,
    pub only: &'a [String],
    pub except: &'a [String],
    /// Under `--raw-config`: the registry is this configuration's cops.
    pub config: Option<&'a Config>,
}

impl crate::directive_comment::CopRegistry for SelectedCops<'_> {
    fn names(&self) -> &[String] {
        self.known.names()
    }

    fn is_department(&self, name: &str) -> bool {
        self.known.is_department(name)
    }

    fn names_for_department(&self, department: &str) -> Vec<String> {
        self.known.names_for_department(department)
    }

    fn enabled_by_option(&self, cop_name: &str) -> bool {
        self.only.iter().any(|name| name == cop_name)
    }

    /// `Runner#mobilize_cop_badge?`: `processed_source.registry` holds only
    /// the cops `--only`/`--except` select.
    fn mobilized(&self, cop_name: &str) -> bool {
        if let Some(config) = self.config {
            // `Registry.global.find_cops_by_directive(key)` for each key: a
            // cop, or every cop of a department.
            let department = cop_name.rsplit_once('/').map(|(d, _)| d);
            return config.get(cop_name).is_some()
                || department.is_some_and(|d| config.get(d).is_some())
                || self.enabled_by_option(cop_name);
        }
        !crate::cop::registry::matches(cop_name, self.except)
            && (self.only.is_empty() || crate::cop::registry::matches(cop_name, self.only))
    }
}

impl Config {
    pub fn new(hash: Mapping, loaded_path: Option<PathBuf>) -> Self {
        let base_dir_for_path_parameters = base_dir_for_path_parameters(loaded_path.as_deref());
        Self {
            hash,
            loaded_path,
            base_dir_for_path_parameters,
            known_cops: OnceLock::new(),
            badge_configs: OnceLock::new(),
            pending_cops_option: None,
            preview_option: None,
            target_ruby: OnceLock::new(),
        }
    }

    /// Port of `Config#for_badge`: the cop's configuration merged into its
    /// department's (with both `Exclude` lists combined).
    pub fn for_badge(&self, cop_name: &str) -> &Mapping {
        let configs = self.badge_configs.get_or_init(|| {
            let mut configs = std::collections::HashMap::new();
            for (key, value) in &self.hash {
                let (Some(name), Some(cop_config)) = (key.as_str(), value.as_mapping()) else {
                    continue;
                };
                let Some((department, _)) = name.rsplit_once('/') else {
                    continue;
                };
                let Some(department_config) = self.hash.get(department).and_then(Value::as_mapping)
                else {
                    continue;
                };
                let mut merged = department_config.clone();
                for (k, v) in cop_config {
                    merged.insert(k.clone(), v.clone());
                }
                if let (Some(Value::Sequence(dept)), Some(Value::Sequence(cop))) =
                    (department_config.get("Exclude"), cop_config.get("Exclude"))
                {
                    let mut union = dept.clone();
                    for item in cop {
                        if !union.contains(item) {
                            union.push(item.clone());
                        }
                    }
                    merged.insert("Exclude".into(), Value::Sequence(union));
                }
                configs.insert(name.to_owned(), merged);
            }
            configs
        });
        configs
            .get(cop_name)
            .unwrap_or_else(|| self.for_cop(cop_name))
    }

    /// The registry directives and directive cops resolve names with:
    /// `Cop::Registry.global` (every core cop, ported or not) plus the cops
    /// this configuration mentions (plugin and custom cops).
    pub fn known_cops(&self) -> &KnownCops {
        self.known_cops.get_or_init(|| {
            let mut known = KnownCops::from_config(&self.hash);
            let core = core_cops();
            known.names.extend(core.names.iter().cloned());
            known.names.sort();
            known.names.dedup();
            known.departments.extend(core.departments.iter().cloned());
            known
        })
    }

    /// `Registry#disabled_names`.
    pub fn disabled_cop_names(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|name| !self.cop_enabled(name))
            .cloned()
            .collect()
    }

    pub fn to_h(&self) -> &Mapping {
        &self.hash
    }

    pub fn loaded_path(&self) -> Option<&Path> {
        self.loaded_path.as_deref()
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir_for_path_parameters
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.hash.get(key)
    }

    pub fn for_all_cops(&self) -> &Mapping {
        self.for_cop("AllCops")
    }

    pub fn for_cop(&self, cop_name: &str) -> &Mapping {
        self.hash
            .get(cop_name)
            .and_then(Value::as_mapping)
            .unwrap_or(&EMPTY)
    }

    pub fn for_department(&self, department: &str) -> &Mapping {
        self.for_cop(department)
    }

    pub fn all_cops_bool(&self, key: &str, default: bool) -> bool {
        self.for_all_cops()
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn enabled_new_cops(&self) -> bool {
        self.for_all_cops().get("NewCops").and_then(Value::as_str) == Some("enable")
    }

    /// `department_of`: the department's configuration.
    fn department_of(&self, cop_name: &str) -> Option<&Mapping> {
        let (department, _) = cop_name.rsplit_once('/')?;
        self.hash.get(department).and_then(Value::as_mapping)
    }

    /// `new_cops_setting_for`: the department's `NewCops`, else `AllCops`'.
    fn new_cops_setting_for(&self, cop_name: &str) -> String {
        let setting = self
            .department_of(cop_name)
            .and_then(|d| d.get("NewCops"))
            .or_else(|| self.for_all_cops().get("NewCops"));
        setting.map(ruby_to_s).unwrap_or_default()
    }

    /// `enabled_new_cop?`.
    pub fn enabled_new_cop(&self, cop_name: &str) -> bool {
        match self.new_cops_setting_for(cop_name).as_str() {
            "enable" => true,
            "" | "pending" | "disable" => false,
            setting => self.new_cops_version_covers(setting, cop_name),
        }
    }

    /// `new_cops_covered?`: the pending status is settled by `NewCops`.
    pub fn new_cops_covered(&self, cop_name: &str) -> bool {
        match self.new_cops_setting_for(cop_name).as_str() {
            "enable" | "disable" => true,
            "" | "pending" => false,
            setting => self.new_cops_version_covers(setting, cop_name),
        }
    }

    fn new_cops_version_covers(&self, pinned: &str, cop_name: &str) -> bool {
        let added = self.for_cop(cop_name).get("VersionAdded").map(ruby_to_s);
        match (
            added.as_deref().and_then(comparable_version),
            comparable_version(pinned),
        ) {
            (Some(added), Some(pinned)) => added <= pinned,
            _ => false,
        }
    }

    /// `Config#target_ruby`: where the target Ruby version comes from.
    pub fn target_ruby(&self) -> &crate::target_ruby::Source {
        self.target_ruby
            .get_or_init(|| crate::target_ruby::source(self))
    }

    /// `Config#bundler_lock_file_path`: the nearest `Gemfile.lock` (else
    /// `gems.locked`) upwards from the base directory, for a configuration
    /// loaded from a file.
    pub fn bundler_lock_file_path(&self) -> Option<PathBuf> {
        self.loaded_path.as_ref()?;
        ["Gemfile.lock", "gems.locked"]
            .into_iter()
            .find_map(|name| {
                crate::file_finder::find_file_upwards(
                    name,
                    &self.base_dir_for_path_parameters,
                    None,
                )
            })
    }

    /// `Config#preview?(options)`: `--[no-]preview`, else `AllCops: Preview`.
    pub fn preview(&self) -> bool {
        self.preview_option
            .unwrap_or_else(|| self.for_all_cops().get("Preview") == Some(&Value::Bool(true)))
    }

    /// `Registry#enabled_pending_cop?`.
    pub fn enabled_pending_cop(&self, cop_name: &str) -> bool {
        match self.pending_cops_option {
            Some(false) => false,
            Some(true) => true,
            None => self.enabled_new_cop(cop_name),
        }
    }

    /// `Config#pending_cops`: cops still `Enabled: pending` that no
    /// `NewCops` setting covers, in configuration order.
    pub fn pending_cops(&self) -> Vec<(&str, &Mapping)> {
        self.hash
            .iter()
            .filter_map(|(key, value)| {
                let name = key.as_str()?;
                let params = value.as_mapping()?;
                if self
                    .department_of(name)
                    .is_some_and(|d| d.get("Enabled") == Some(&Value::Bool(false)))
                {
                    return None;
                }
                if params.get("Enabled").and_then(Value::as_str) != Some("pending")
                    || self.new_cops_covered(name)
                {
                    return None;
                }
                Some((name, params))
            })
            .collect()
    }

    /// `Config#cop_enabled?` alone: `!!for_cop(name)['Enabled']`, where
    /// `for_cop` computes `Enabled` with `enable_cop?` (so `pending` is
    /// truthy here).
    pub fn cop_enabled_in_config(&self, cop_name: &str) -> bool {
        let enabled = self.for_cop(cop_name).get("Enabled");
        if enabled == Some(&Value::Bool(true)) || cop_name == "Lint/Syntax" {
            return true;
        }
        if enabled.and_then(Value::as_str) == Some("override_department") {
            return true;
        }
        if self
            .department_of(cop_name)
            .is_some_and(|d| d.get("Enabled") == Some(&Value::Bool(false)))
        {
            return false;
        }
        match enabled {
            None => !self.all_cops_bool("DisabledByDefault", false),
            Some(Value::Bool(b)) => *b,
            Some(Value::Null) => false,
            Some(_) => true,
        }
    }

    /// `Config#for_enabled_cop`: the cop's configuration if
    /// `cop_enabled?`, else an empty one.
    pub fn for_enabled_cop(&self, cop_name: &str) -> &Mapping {
        if self.cop_enabled_in_config(cop_name) {
            self.for_cop(cop_name)
        } else {
            &EMPTY
        }
    }

    /// Port of `Config#cop_enabled?` combined with `Registry#enabled?`.
    pub fn cop_enabled(&self, cop_name: &str) -> bool {
        let options = self.for_cop(cop_name);
        let enabled = options.get("Enabled");
        if enabled == Some(&Value::Bool(true)) || cop_name == "Lint/Syntax" {
            return true;
        }
        if enabled.and_then(Value::as_str) == Some("override_department") {
            return true;
        }
        if self
            .department_of(cop_name)
            .is_some_and(|d| d.get("Enabled") == Some(&Value::Bool(false)))
        {
            return false;
        }
        match enabled {
            Some(Value::String(s)) if s == "pending" => self.enabled_pending_cop(cop_name),
            // `enabled_preview_cop?`
            Some(Value::String(s)) if s == "preview" => self.preview(),
            Some(Value::Bool(b)) => *b,
            Some(_) => true,
            None => !self.all_cops_bool("DisabledByDefault", false),
        }
    }

    pub fn path_relative_to_config(&self, path: &Path) -> String {
        path_util::relative_path(path, &self.base_dir_for_path_parameters)
    }

    pub fn file_to_include(&self, file: &Path) -> bool {
        self.file_to_include_by(file, |_| true)
    }

    /// `file_to_include?(file) { |pattern| ... }`: only the patterns the
    /// block accepts are tried.
    fn file_to_include_by(&self, file: &Path, accept: impl Fn(&str) -> bool) -> bool {
        let relative = self.path_relative_to_config(file);
        let is_hidden = relative.starts_with('.') && !relative.starts_with("..");
        if is_hidden && !self.possibly_include_hidden() {
            return false;
        }
        let absolute = path_util::expand_path(file).to_string_lossy().into_owned();
        patterns(self.for_all_cops(), "Include")
            .filter(|pattern| accept(pattern))
            .any(|pattern| match_path(pattern, &relative) || match_path(pattern, &absolute))
    }

    /// `allowed_camel_case_file?(file)`.
    pub fn allowed_camel_case_file(&self, file: &Path) -> bool {
        // Gemspecs are allowed to have dashes because that fits with bundler
        // best practices in the case when the gem is nested under a namespace
        // (e.g., `bundler-console` conveys `Bundler::Console`).
        if file.extension().is_some_and(|e| e == "gemspec") {
            return true;
        }
        self.file_to_include_by(file, |pattern| {
            pattern.bytes().any(|b| b.is_ascii_uppercase())
        })
    }

    pub fn file_to_exclude(&self, file: &Path) -> bool {
        let absolute = path_util::expand_path(file).to_string_lossy().into_owned();
        patterns(self.for_all_cops(), "Exclude").any(|pattern| match_path(pattern, &absolute))
    }

    fn possibly_include_hidden(&self) -> bool {
        patterns(self.for_all_cops(), "Include")
            .any(|pattern| pattern.starts_with('.') || pattern.contains("/."))
    }

    /// Port of `Base#relevant_file?` / `file_name_matches_any?` for a cop's
    /// own `Include` and `Exclude` parameters.
    pub fn cop_relevant_file(&self, cop_name: &str, file: &Path) -> bool {
        if file
            .file_name()
            .is_some_and(|name| name == STRING_SOURCE_NAME)
        {
            return true;
        }
        let cop_config = self.for_badge(cop_name);
        let relative = self.path_relative_to_config(file);
        let absolute = file.to_string_lossy();
        let matches_any = |key: &str, default: bool| match cop_config.get(key) {
            Some(Value::Sequence(_)) => patterns(cop_config, key)
                .any(|pattern| match_path(pattern, &relative) || match_path(pattern, &absolute)),
            _ => default,
        };
        matches_any("Include", true) && !matches_any("Exclude", false)
    }

    /// Port of `Config#make_excludes_absolute`.
    pub fn make_excludes_absolute(&mut self) {
        let base = self.base_dir_for_path_parameters.clone();
        for (_, value) in self.hash.iter_mut() {
            let Some(Value::Sequence(excludes)) =
                value.as_mapping_mut().and_then(|m| m.get_mut("Exclude"))
            else {
                continue;
            };
            for exclude in excludes {
                if let Value::String(pattern) = exclude
                    && !path_util::absolute(pattern)
                {
                    *pattern = path_util::expand_path(&base.join(&*pattern))
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
    }
}

fn patterns<'a>(mapping: &'a Mapping, key: &str) -> impl Iterator<Item = &'a str> {
    mapping
        .get(key)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn base_dir_for_path_parameters(loaded_path: Option<&Path>) -> PathBuf {
    if let Some(path) = loaded_path {
        let is_dotfile = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".rubocop"));
        let home_dotfile = std::env::home_dir().map(|home| home.join(".rubocop.yml"));
        if is_dotfile && home_dotfile.as_deref() != Some(path) {
            return path_util::expand_path(path.parent().unwrap_or(Path::new(".")));
        }
    }
    path_util::pwd()
}

/// Ruby's `to_s` for a YAML scalar (`1.19` -> "1.19").
fn ruby_to_s(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// `comparable_version`: `Gem::Version` for values like `1.19`, `None` for
/// `N/A` or `<<next>>`. Compared segment by segment (numeric segments only).
fn comparable_version(value: &str) -> Option<Vec<u64>> {
    if !value.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let mut segments: Vec<u64> = value
        .split('.')
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    while segments.len() > 1 && segments.last() == Some(&0) {
        segments.pop();
    }
    Some(segments)
}
