//! Port of `lib/rubocop/config_loader.rb`, `config_loader_resolver.rb` and
//! `config_finder.rb`.
//!
//! Plugins (`plugins:`, and `require:` of a plugin gem) are integrated as in
//! `plugin/configuration_integrator.rb`, from the gems the Ruby launcher lists
//! in `OXCOP_GEMS` (see `Gems`).
//!
//! ERB in config files is rendered by Ruby (see `render_erb`).
//!
//! Not yet ported: remote `inherit_from` URLs, `inherit_from` globs,
//! `DisabledByDefault` handling of user-enabled cops.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use serde_yaml_ng::{Mapping, Value};

use crate::config::Config;
use crate::path_util;

pub const DOTFILE: &str = ".rubocop.yml";

const DEFAULT_YML: &str = include_str!("../../../vendor/rubocop/config/default.yml");

static DEFAULT_CONFIGURATION: LazyLock<Mapping> = LazyLock::new(|| {
    serde_yaml_ng::from_str(DEFAULT_YML).expect("vendored default.yml is valid YAML")
});

#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn default_configuration() -> &'static Mapping {
    &DEFAULT_CONFIGURATION
}

/// Port of `ConfigFinder.find_config_path`.
pub fn find_config_path(target_dir: &Path) -> Option<PathBuf> {
    for dir in target_dir.ancestors() {
        let candidate = dir.join(DOTFILE);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let home = std::env::home_dir()?;
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    [home.join(DOTFILE), xdg.join("rubocop/config.yml")]
        .into_iter()
        .find(|path| path.is_file())
}

/// The gems the launcher (`gems/oxcop/lib/oxcop.rb`) found: under Bundler
/// the bundle's, otherwise the latest installed version of each. One line per
/// gem: `name<TAB>full_gem_path<TAB>default_lint_roller_plugin`.
#[derive(Debug, Default, Clone)]
pub struct Gems {
    /// `None` when the binary runs without the launcher.
    by_name: Option<HashMap<String, GemSpec>>,
}

#[derive(Debug, Clone)]
struct GemSpec {
    dir: PathBuf,
    lint_roller_plugin: bool,
}

impl Gems {
    pub const ENV: &'static str = "OXCOP_GEMS";

    pub fn from_env() -> Self {
        std::env::var(Self::ENV)
            .map(|list| Self::parse(&list))
            .unwrap_or_default()
    }

    pub fn parse(list: &str) -> Self {
        let by_name = list
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let name = fields.next()?;
                let dir = fields.next()?;
                let plugin = fields.next().is_some_and(|class| !class.is_empty());
                Some((
                    name.to_owned(),
                    GemSpec {
                        dir: PathBuf::from(dir),
                        lint_roller_plugin: plugin,
                    },
                ))
            })
            .collect();
        Self {
            by_name: Some(by_name),
        }
    }

    /// `Plugin.plugin_capable?`.
    fn plugin_capable(&self, name: &str) -> bool {
        self.by_name
            .as_ref()
            .and_then(|gems| gems.get(name))
            .is_some_and(|gem| gem.lint_roller_plugin)
    }

    /// The plugin's rules file. Every plugin we know of returns
    /// `config/default.yml` from `rules`; `type: :object` rules aren't
    /// supported.
    fn plugin_config_path(&self, name: &str) -> Option<PathBuf> {
        let gem = self.by_name.as_ref()?.get(name)?;
        gem.lint_roller_plugin
            .then(|| gem.dir.join("config").join("default.yml"))
    }
}

/// `ConfigLoader`'s class-level state, scoped to one run: the default
/// configuration (which plugins extend) and the plugins loaded so far.
#[derive(Debug, Default)]
pub struct ConfigLoader {
    default_configuration: Option<Mapping>,
    loaded_plugins: HashSet<String>,
    pub gems: Gems,
}

impl ConfigLoader {
    pub fn new(gems: Gems) -> Self {
        Self {
            gems,
            ..Default::default()
        }
    }

    /// `ConfigLoader.default_configuration`: default.yml with its `Exclude`s
    /// made absolute (relative to the working directory, as default.yml isn't
    /// a `.rubocop*` file), plus any integrated plugins.
    pub fn default_configuration(&mut self) -> &Mapping {
        self.default_configuration.get_or_insert_with(|| {
            let mut config = Config::new(default_configuration().clone(), None);
            config.make_excludes_absolute();
            config.to_h().clone()
        })
    }

    /// Port of `ConfigLoader.configuration_from_file`.
    pub fn configuration_from_file(
        &mut self,
        config_file: Option<&Path>,
    ) -> Result<Config, ConfigError> {
        let Some(config_file) = config_file else {
            let defaults = self.default_configuration().clone();
            return Ok(Config::new(apply_preview_defaults(&defaults, false), None));
        };
        let config_file = path_util::expand_path(config_file);
        let hash = self.load_file(&config_file)?;
        let merged = self.merge_with_default(hash, true);
        Ok(Config::new(merged, Some(config_file)))
    }

    /// Port of `ConfigLoader.load_file`: reads one file and resolves its
    /// plugins and `inherit_from` chain. `Exclude` patterns are made absolute
    /// relative to the file that declared them.
    pub fn load_file(&mut self, path: &Path) -> Result<Mapping, ConfigError> {
        self.load_file_checked(path, true)
    }

    fn load_file_checked(&mut self, path: &Path, check: bool) -> Result<Mapping, ConfigError> {
        let mut hash = load_yaml_configuration(path)?;

        let plugins = hash.shift_remove("plugins");
        self.resolve_plugins(plugins.as_ref(), "plugins")?;
        self.resolve_requires(path, &mut hash)?;
        self.resolve_inheritance_from_gems(&mut hash)?;
        self.resolve_inheritance(path, &mut hash)?;
        hash.shift_remove("inherit_from");

        if check {
            let mut own = Config::new(hash, Some(path.to_path_buf()));
            own.make_excludes_absolute();
            hash = own.to_h().clone();
        }
        Ok(hash)
    }

    /// `ConfigLoaderResolver#resolve_plugins`, then `Plugin.integrate_plugins`.
    fn resolve_plugins(&mut self, plugins: Option<&Value>, key: &str) -> Result<(), ConfigError> {
        let mut names = Vec::new();
        for plugin in array(plugins) {
            match plugin {
                Value::String(name) => names.push(name.clone()),
                // `- rubocop-foo: { enabled: false, require_path: ..., ... }`
                Value::Mapping(m) => {
                    let Some((Value::String(name), config)) = m.iter().next() else {
                        continue;
                    };
                    let enabled = config
                        .as_mapping()
                        .and_then(|c| c.get("enabled"))
                        .is_none_or(truthy);
                    if enabled {
                        names.push(name.clone());
                    }
                }
                _ => {}
            }
        }
        names.retain(|name| !self.loaded_plugins.contains(name));
        if names.is_empty() {
            return Ok(());
        }
        if self.gems.by_name.is_none() {
            warn_once(&format!(
                "Ruby extensions in `{key}` are not supported and will be ignored: {}",
                names.join(", ")
            ));
            return Ok(());
        }
        let mut paths = Vec::new();
        for name in &names {
            let path = self
                .gems
                .plugin_config_path(name)
                .ok_or_else(|| ConfigError(plugin_load_error(name)))?;
            paths.push(path);
        }
        self.integrate_plugins_into_rubocop_config(&paths)?;
        self.loaded_plugins.extend(names);
        Ok(())
    }

    /// `ConfigLoaderResolver#resolve_requires`. Ruby features can't be
    /// loaded; plugin gems named in `require:` are integrated as plugins.
    fn resolve_requires(&mut self, path: &Path, hash: &mut Mapping) -> Result<(), ConfigError> {
        let Some(features) = hash.shift_remove("require") else {
            return Ok(());
        };
        let mut ignored = Vec::new();
        for feature in array(Some(&features)) {
            let Value::String(feature) = feature else {
                continue;
            };
            if self.gems.plugin_capable(&feature) {
                warn(&format!(
                    "{feature} extension supports plugin, specify `plugins: {feature}` instead of \
                     `require: {feature}` in {}.\nFor more information, see \
                     https://docs.rubocop.org/rubocop/plugin_migration_guide.html.",
                    path.display()
                ));
                self.resolve_plugins(Some(&Value::String(feature.clone())), "require")?;
            } else {
                ignored.push(feature.clone());
            }
        }
        if !ignored.is_empty() {
            warn_once(&format!(
                "Ruby extensions in `require` are not supported and will be ignored: {}",
                ignored.join(", ")
            ));
        }
        Ok(())
    }

    /// Port of `ConfigLoaderResolver#resolve_inheritance_from_gems`: each
    /// `inherit_gem` entry is prepended to `inherit_from`.
    fn resolve_inheritance_from_gems(&self, hash: &mut Mapping) -> Result<(), ConfigError> {
        let Some(Value::Mapping(gems)) = hash.shift_remove("inherit_gem") else {
            return Ok(());
        };
        for (gem_name, config_path) in &gems {
            let Some(gem_name) = gem_name.as_str() else {
                continue;
            };
            if gem_name == "rubocop" {
                return Err(ConfigError(
                    "can't inherit configuration from the rubocop gem".to_owned(),
                ));
            }
            let Some(gem_dir) = self.gem_dir(gem_name)? else {
                continue;
            };
            let mut inherit_from = array(hash.get("inherit_from"));
            for path in array(Some(config_path)).iter().rev() {
                let Some(path) = path.as_str() else {
                    continue;
                };
                let full = gem_dir.join(path).to_string_lossy().into_owned();
                inherit_from.insert(0, Value::String(full));
            }
            hash.insert("inherit_from".into(), Value::Sequence(inherit_from));
        }
        Ok(())
    }

    /// The gem's directory for `gem_config_path`; `None` (with a warning)
    /// when the binary runs without the launcher and so knows no gems.
    fn gem_dir(&self, gem_name: &str) -> Result<Option<PathBuf>, ConfigError> {
        let Some(gems) = &self.gems.by_name else {
            warn_once(&format!(
                "`inherit_gem` needs the oxcop gem's launcher to find gems; ignoring {gem_name}"
            ));
            return Ok(None);
        };
        match gems.get(gem_name) {
            Some(gem) => Ok(Some(gem.dir.clone())),
            None => Err(ConfigError(format!(
                "Unable to find gem {gem_name}; is the gem installed?"
            ))),
        }
    }

    /// Port of `Plugin::ConfigurationIntegrator.integrate_plugins_into_rubocop_config`.
    fn integrate_plugins_into_rubocop_config(
        &mut self,
        plugin_paths: &[PathBuf],
    ) -> Result<(), ConfigError> {
        let mut default_config = self.default_configuration().clone();
        let plugin_config = self.combine_rubocop_configs(&default_config, plugin_paths)?;

        // merge_plugin_config_into_all_cops!
        if let (Some(Value::Mapping(all_cops)), Some(Value::Mapping(plugin_all_cops))) = (
            default_config.get_mut("AllCops"),
            plugin_config.get("AllCops"),
        ) {
            for (key, value) in plugin_all_cops {
                all_cops.insert(key.clone(), value.clone());
            }
        }
        // merge_plugin_config_into_default_config!
        for (key, value) in &plugin_config {
            let merged = match (default_config.get(key), value) {
                (Some(Value::Mapping(base)), Value::Mapping(derived)) => {
                    Value::Mapping(merge(base, derived))
                }
                _ => value.clone(),
            };
            default_config.insert(key.clone(), merged);
        }
        self.default_configuration = Some(default_config);
        Ok(())
    }

    fn combine_rubocop_configs(
        &mut self,
        default_config: &Mapping,
        plugin_paths: &[PathBuf],
    ) -> Result<Mapping, ConfigError> {
        let original = self.default_configuration.take();
        let mut combined = default_config.clone();
        let mut configured_keys = Vec::new();
        let result = (|| {
            for path in plugin_paths {
                // Each plugin's config merges onto the defaults combined so far.
                self.default_configuration = Some(combined.clone());
                let mut plugin_config = self.load_file_checked(path, false)?;
                let all_cops = merge_all_cop_settings(
                    combined.get("AllCops"),
                    plugin_config.get("AllCops"),
                    &mut configured_keys,
                );
                plugin_config.insert("AllCops".into(), all_cops);
                let mut plugin = Config::new(plugin_config, Some(path.clone()));
                plugin.make_excludes_absolute();
                combined = self.merge_with_default(plugin.to_h().clone(), false);
            }
            Ok(combined)
        })();
        self.default_configuration = original;
        result
    }

    /// Port of `ConfigLoaderResolver#resolve_inheritance`.
    fn resolve_inheritance(&mut self, path: &Path, hash: &mut Mapping) -> Result<(), ConfigError> {
        let inherited_files: Vec<String> = array(hash.get("inherit_from"))
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let base_dir = path.parent().unwrap_or(Path::new("."));
        let mut base_configs = Vec::new();
        for file in &inherited_files {
            if file.starts_with("http://") || file.starts_with("https://") {
                warn_once(&format!(
                    "remote inherit_from ({file}) is not supported yet; skipping"
                ));
                continue;
            }
            let inherited = path_util::expand_path(&base_dir.join(file));
            base_configs.push((self.load_file(&inherited)?, inherited));
        }

        for (base_config, base_path) in base_configs.iter().rev() {
            override_department_setting_for_cops(base_config, hash);
            override_enabled_for_disabled_departments(base_config, hash);
            for (key, value) in base_config {
                let Value::Mapping(base_value) = value else {
                    continue;
                };
                let derived = hash.get(key);
                let only_base_has_include = base_value.contains_key("Include")
                    && !derived
                        .and_then(Value::as_mapping)
                        .and_then(|m| m.get("Include"))
                        .is_some_and(truthy);
                let value = match derived {
                    Some(Value::Mapping(derived)) => {
                        let mode = determine_inherit_mode(hash, key);
                        Value::Mapping(merge_with(
                            base_value,
                            derived,
                            &MergeOptions {
                                inherit_mode: Some(&mode),
                                unset_nil: false,
                            },
                        ))
                    }
                    Some(other) => other.clone(),
                    None => value.clone(),
                };
                hash.insert(key.clone(), value);
                if only_base_has_include {
                    fix_include_paths(base_path, hash, path, key);
                }
            }
        }
        Ok(())
    }

    /// Port of `ConfigLoaderResolver#merge_with_default`.
    fn merge_with_default(&mut self, mut config: Mapping, unset_nil: bool) -> Mapping {
        let mut defaults = apply_preview_defaults(self.default_configuration(), false);
        let all_cops = config.get("AllCops").and_then(Value::as_mapping);
        let flag = |key| all_cops.and_then(|m| m.get(key)).is_some_and(truthy);
        let (disabled_by_default, enabled_by_default) =
            (flag("DisabledByDefault"), flag("EnabledByDefault"));
        if disabled_by_default || enabled_by_default {
            for (key, params) in defaults.iter_mut() {
                if key.as_str().is_some_and(|k| k.contains('/'))
                    && let Some(params) = params.as_mapping_mut()
                {
                    params.insert("Enabled".into(), Value::Bool(!disabled_by_default));
                }
            }
        }
        override_enabled_for_disabled_departments(&defaults, &mut config);
        let mode = config
            .get("inherit_mode")
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        merge_with(
            &defaults,
            &config,
            &MergeOptions {
                inherit_mode: Some(&mode),
                unset_nil,
            },
        )
    }
}

fn plugin_load_error(name: &str) -> String {
    format!(
        "Failed to load plugin `{name}` because the corresponding plugin class could not be \
         determined for instantiation.\nTry upgrading it first (e.g., `bundle update {name}`).\n\
         If `{name}` is not yet a plugin, use `require: {name}` instead of `plugins: {name}` in \
         your configuration.\n\nFor further assistance, check with the developer regarding the \
         following points:\nhttps://docs.rubocop.org/rubocop/plugin_migration_guide.html"
    )
}

/// `ConfigurationIntegrator.merge_all_cop_settings`: first plugin in wins
/// for each `AllCops` key, except that arrays (`Include`, `Exclude`, ...)
/// are unioned.
fn merge_all_cop_settings(
    existing: Option<&Value>,
    new: Option<&Value>,
    configured_keys: &mut Vec<Value>,
) -> Value {
    let mut combined = existing
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    let Some(Value::Mapping(new)) = new else {
        return Value::Mapping(combined);
    };
    for (key, value) in new {
        match (combined.get(key), value) {
            (Some(Value::Sequence(existing)), Value::Sequence(added)) => {
                let union = union(existing, added);
                combined.insert(key.clone(), Value::Sequence(union));
                if !configured_keys.contains(key) {
                    configured_keys.push(key.clone());
                }
            }
            _ if !configured_keys.contains(key) => {
                combined.insert(key.clone(), value.clone());
                configured_keys.push(key.clone());
            }
            _ => {}
        }
    }
    Value::Mapping(combined)
}

/// Port of `ConfigLoader.load_yaml_configuration`.
fn load_yaml_configuration(path: &Path) -> Result<Mapping, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        ConfigError(format!(
            "Configuration file not found: {} ({e})",
            path.display()
        ))
    })?;
    let contents = render_erb(contents, path)?;
    let value: Value = serde_yaml_ng::from_str(&contents)
        .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
    match value {
        Value::Mapping(mapping) => Ok(mapping),
        Value::Null => Ok(Mapping::new()),
        _ => Err(ConfigError(format!(
            "Malformed configuration in {}",
            path.display()
        ))),
    }
}

/// `Dir.chdir(File.dirname(path)) { ERB.new(file_contents).result }`. ERB
/// is Ruby, so a file with ERB tags is rendered by `ruby -rerb` (the
/// launcher's Ruby, `OXCOP_RUBY`, or `ruby` on PATH). A file without tags
/// renders to itself.
fn render_erb(contents: String, path: &Path) -> Result<String, ConfigError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if !contents.contains("<%") {
        return Ok(contents);
    }
    let ruby = std::env::var_os("OXCOP_RUBY").unwrap_or_else(|| "ruby".into());
    let error = |message: String| ConfigError(format!("{}: ERB: {message}", path.display()));
    let mut child = Command::new(&ruby)
        .args(["-rerb", "-e", "print ERB.new($stdin.read).result"])
        .current_dir(path.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| error(format!("running {}: {e}", ruby.to_string_lossy())))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || stdin.write_all(contents.as_bytes()));
    let output = child.wait_with_output().map_err(|e| error(e.to_string()))?;
    let _ = writer.join();
    if !output.status.success() {
        return Err(error(String::from_utf8_lossy(&output.stderr).into_owned()));
    }
    String::from_utf8(output.stdout).map_err(|e| error(e.to_string()))
}

/// Port of `ConfigLoaderResolver#fix_include_paths`.
fn fix_include_paths(base_config_path: &Path, hash: &mut Mapping, path: &Path, key: &Value) {
    if !base_config_path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(".rubocop"))
    {
        return;
    }
    let base_dir = base_config_path.parent().unwrap_or(Path::new("."));
    let derived_dir = path.parent().unwrap_or(Path::new("."));
    let Some(Value::Mapping(params)) = hash.get_mut(key) else {
        return;
    };
    let Some(Value::Sequence(includes)) = params.get_mut("Include") else {
        return;
    };
    for include in includes {
        if let Value::String(pattern) = include {
            let joined = path_util::expand_path(&base_dir.join(&*pattern));
            *pattern = path_util::relative_path(&joined, derived_dir);
        }
    }
}

/// `ConfigLoaderResolver#override_department_setting_for_cops`.
fn override_department_setting_for_cops(base: &Mapping, derived: &mut Mapping) {
    let keys: Vec<Value> = derived
        .keys()
        .filter(|key| {
            key.as_str()
                .and_then(|k| k.rsplit_once('/'))
                .is_some_and(|(department, _)| {
                    department_disabled(derived, department)
                        || department_disabled(base, department)
                })
        })
        .cloned()
        .collect();
    for key in keys {
        if let Some(Value::Mapping(params)) = derived.get_mut(&key)
            && params.get("Enabled").is_some_and(truthy)
        {
            params.insert("Enabled".into(), "override_department".into());
        }
    }
}

/// `ConfigLoaderResolver#determine_inherit_mode`.
fn determine_inherit_mode(hash: &Mapping, key: &Value) -> Mapping {
    let local = hash
        .get(key)
        .and_then(Value::as_mapping)
        .and_then(|cop| cop.get("inherit_mode"));
    local
        .or_else(|| hash.get("inherit_mode"))
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default()
}

/// Ruby truthiness.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// `Array(value)`.
fn array(value: Option<&Value>) -> Vec<Value> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Sequence(items)) => items.clone(),
        Some(other) => vec![other.clone()],
    }
}

/// `Array#|`.
fn union(base: &[Value], derived: &[Value]) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::new();
    for value in base.iter().chain(derived) {
        if !result.contains(value) {
            result.push(value.clone());
        }
    }
    result
}

fn apply_preview_defaults(defaults: &Mapping, preview: bool) -> Mapping {
    let mut result = defaults.clone();
    for (_, params) in result.iter_mut() {
        let Some(params) = params.as_mapping_mut() else {
            continue;
        };
        if let Some(Value::Mapping(preview_params)) = params.shift_remove("Preview")
            && preview
        {
            *params = merge(params, &preview_params);
        }
    }
    result
}

fn department_disabled(hash: &Mapping, department: &str) -> bool {
    hash.get(department)
        .and_then(Value::as_mapping)
        .and_then(|m| m.get("Enabled"))
        == Some(&Value::Bool(false))
}

fn override_enabled_for_disabled_departments(base: &Mapping, derived: &mut Mapping) {
    let departments: Vec<String> = derived
        .keys()
        .filter_map(Value::as_str)
        .filter(|key| department_disabled(derived, key))
        .map(|key| format!("{key}/"))
        .collect();
    for (cop_name, params) in base {
        let Some(cop_name) = cop_name.as_str() else {
            continue;
        };
        if !departments
            .iter()
            .any(|prefix| cop_name.starts_with(prefix))
        {
            continue;
        }
        if params.as_mapping().and_then(|m| m.get("Enabled")) != Some(&Value::Bool(true)) {
            continue;
        }
        let mut disabled = Mapping::new();
        let mut params = Mapping::new();
        params.insert("Enabled".into(), Value::Bool(false));
        disabled.insert(cop_name.into(), Value::Mapping(params));
        *derived = merge(&disabled, derived);
    }
}

#[derive(Default)]
struct MergeOptions<'a> {
    inherit_mode: Option<&'a Mapping>,
    unset_nil: bool,
}

/// Port of `ConfigLoaderResolver#merge`: a recursive hash merge.
pub fn merge(base: &Mapping, derived: &Mapping) -> Mapping {
    merge_with(base, derived, &MergeOptions::default())
}

fn merge_with(base: &Mapping, derived: &Mapping, opts: &MergeOptions<'_>) -> Mapping {
    let mut result = base.clone();
    for (key, value) in derived {
        result.insert(key.clone(), value.clone());
    }
    for (key, base_value) in base {
        let Some(derived_value) = derived.get(key) else {
            continue;
        };
        if opts.unset_nil && derived_value.is_null() {
            result.shift_remove(key);
        } else if let (Value::Mapping(b), Value::Mapping(d)) = (base_value, derived_value) {
            result.insert(key.clone(), Value::Mapping(merge_with(b, d, opts)));
        } else if should_union(derived, base, opts.inherit_mode, key) {
            let union = union(&array(Some(base_value)), &array(Some(derived_value)));
            result.insert(key.clone(), Value::Sequence(union));
        }
    }
    result
}

/// `ConfigLoaderResolver#should_union?`.
fn should_union(
    derived: &Mapping,
    base: &Mapping,
    root_mode: Option<&Mapping>,
    key: &Value,
) -> bool {
    if !base.get(key).is_some_and(Value::is_sequence)
        && !derived.get(key).is_some_and(Value::is_sequence)
    {
        return false;
    }
    let mode_of = |hash: &Mapping| {
        hash.get("inherit_mode")
            .and_then(Value::as_mapping)
            .cloned()
    };
    for mode in [mode_of(derived), mode_of(base)].into_iter().flatten() {
        if mode_includes(&mode, "override", key) {
            return false;
        }
        if mode_includes(&mode, "merge", key) {
            return true;
        }
    }
    root_mode.is_some_and(|mode| mode_includes(mode, "merge", key))
}

fn mode_includes(mode: &Mapping, kind: &str, key: &Value) -> bool {
    match (mode.get(kind), key) {
        (Some(Value::Sequence(keys)), _) => keys.contains(key),
        (Some(Value::String(keys)), Value::String(key)) => keys.contains(key.as_str()),
        _ => false,
    }
}

thread_local! {
    static SEEN_WARNINGS: RefCell<std::collections::HashSet<String>> = RefCell::default();
}

fn warn(message: &str) {
    use std::io::Write;
    let _ = writeln!(crate::cli::stderr(), "{message}");
}

fn warn_once(message: &str) {
    SEEN_WARNINGS.with(|seen| {
        if seen.borrow_mut().insert(message.to_owned()) {
            use std::io::Write;
            let _ = writeln!(crate::cli::stderr(), "oxcop: {message}");
        }
    });
}

/// Each run warns once per message (`--server` handles many runs).
pub fn reset_warnings() {
    SEEN_WARNINGS.with(|seen| seen.borrow_mut().clear());
}

/// Port of `ConfigStore`: caches the resolved configuration per config file
/// and finds the right one for each inspected file.
#[derive(Default)]
pub struct ConfigStore {
    options_config: Option<PathBuf>,
    /// `--raw-config`: use the `-c` file as the complete configuration, like
    /// `RuboCop::Config.new(hash)` in RuboCop's own specs (no defaults merged).
    pub raw: bool,
    by_path: HashMap<Option<PathBuf>, Rc<Config>>,
    by_dir: HashMap<PathBuf, Option<PathBuf>>,
    loader: ConfigLoader,
}

impl ConfigStore {
    pub fn new(options_config: Option<PathBuf>) -> Self {
        Self {
            options_config: options_config.map(|p| path_util::expand_path(&p)),
            loader: ConfigLoader::new(Gems::from_env()),
            ..Default::default()
        }
    }

    pub fn for_file(&mut self, file: &Path) -> Result<Rc<Config>, ConfigError> {
        self.for_dir(file.parent().unwrap_or(Path::new(".")))
    }

    pub fn for_pwd(&mut self) -> Result<Rc<Config>, ConfigError> {
        self.for_dir(&path_util::pwd())
    }

    pub fn for_dir(&mut self, dir: &Path) -> Result<Rc<Config>, ConfigError> {
        let config_path = match &self.options_config {
            Some(path) => Some(path.clone()),
            None => self
                .by_dir
                .entry(dir.to_path_buf())
                .or_insert_with(|| find_config_path(dir))
                .clone(),
        };
        if !self.by_path.contains_key(&config_path) {
            let config = if self.raw
                && let Some(path) = &config_path
            {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
                let hash: Mapping = serde_yaml_ng::from_str(&contents)
                    .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
                Config::new(hash, Some(path.clone()))
            } else {
                self.loader
                    .configuration_from_file(config_path.as_deref())?
            };
            self.by_path.insert(config_path.clone(), Rc::new(config));
        }
        Ok(Rc::clone(&self.by_path[&config_path]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(source: &str) -> Mapping {
        serde_yaml_ng::from_str(source).unwrap()
    }

    #[test]
    fn inherit_mode_merge_unions_arrays() {
        let base = yaml("AllCops:\n  Exclude: [a, b]\n");
        let derived = yaml("AllCops:\n  Exclude: [b, c]\n");
        let mode = yaml("merge: [Exclude]\n");
        let merged = merge_with(
            &base,
            &derived,
            &MergeOptions {
                inherit_mode: Some(&mode),
                unset_nil: true,
            },
        );
        assert_eq!(merged, yaml("AllCops:\n  Exclude: [a, b, c]\n"));
        assert_eq!(merge(&base, &derived), derived);
    }

    #[test]
    fn local_inherit_mode_override_wins() {
        let base = yaml("Foo/Bar:\n  Exclude: [a]\n");
        let derived = yaml("Foo/Bar:\n  inherit_mode: {override: [Exclude]}\n  Exclude: [b]\n");
        let mode = yaml("merge: [Exclude]\n");
        let merged = merge_with(
            &base,
            &derived,
            &MergeOptions {
                inherit_mode: Some(&mode),
                unset_nil: false,
            },
        );
        assert_eq!(
            merged["Foo/Bar"]["Exclude"],
            serde_yaml_ng::from_str::<Value>("[b]").unwrap()
        );
    }

    #[test]
    fn all_cops_settings_first_plugin_wins_and_arrays_union() {
        let mut keys = Vec::new();
        let defaults: Value =
            serde_yaml_ng::from_str("{Exclude: [a], TargetRailsVersion: ~}").unwrap();
        let first: Value =
            serde_yaml_ng::from_str("{Exclude: [b], TargetRailsVersion: 7}").unwrap();
        let combined = merge_all_cop_settings(Some(&defaults), Some(&first), &mut keys);
        let second: Value =
            serde_yaml_ng::from_str("{Exclude: [c], TargetRailsVersion: 8}").unwrap();
        let combined = merge_all_cop_settings(Some(&combined), Some(&second), &mut keys);
        assert_eq!(
            combined,
            serde_yaml_ng::from_str::<Value>("{Exclude: [a, b, c], TargetRailsVersion: 7}")
                .unwrap()
        );
    }

    #[test]
    fn plugins_extend_a_per_run_default_configuration() {
        let dir = std::env::temp_dir().join(format!("oxcop-plugin-test-{}", std::process::id()));
        let gem = dir.join("rubocop-fake");
        std::fs::create_dir_all(gem.join("config")).unwrap();
        std::fs::write(
            gem.join("config/default.yml"),
            "AllCops:\n  Exclude: ['bin/*']\nFake/Cop:\n  Enabled: true\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(".rubocop.yml"),
            "inherit_mode:\n  merge: [Exclude]\nplugins: [rubocop-fake]\nAllCops:\n  Exclude: ['tmp/**/*']\n",
        )
        .unwrap();
        let gems = Gems::parse(&format!("rubocop-fake\t{}\tFake::Plugin\n", gem.display()));
        let mut loader = ConfigLoader::new(gems);
        let config = loader
            .configuration_from_file(Some(&dir.join(".rubocop.yml")))
            .unwrap();
        assert_eq!(config.for_cop("Fake/Cop")["Enabled"], Value::Bool(true));
        let excludes = config.for_all_cops()["Exclude"].as_sequence().unwrap();
        assert!(
            excludes
                .iter()
                .any(|e| e.as_str().unwrap().ends_with("/bin/*"))
        );
        assert!(
            excludes
                .iter()
                .any(|e| e.as_str().unwrap().ends_with("/tmp/**/*"))
        );
        assert!(!default_configuration().contains_key("Fake/Cop"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn erb_is_rendered_in_the_config_directory_and_inherit_gem_resolves() {
        let dir = std::env::temp_dir().join(format!("oxcop-erb-test-{}", std::process::id()));
        let gem = dir.join("style-gem");
        std::fs::create_dir_all(gem.join("config")).unwrap();
        std::fs::write(gem.join("config/style.yml"), "Style/Foo:\n  Max: 3\n").unwrap();
        std::fs::write(dir.join("version.txt"), "3.4.1\n").unwrap();
        std::fs::write(
            dir.join(".rubocop.yml"),
            "inherit_gem:\n  style-gem: config/style.yml\nAllCops:\n  TargetRubyVersion: \
             <%= File.read('version.txt')[/\\d+\\.\\d+/] %>\nStyle/Foo:\n  Enabled: true\n",
        )
        .unwrap();
        let gems = Gems::parse(&format!("style-gem\t{}\t\n", gem.display()));
        let mut loader = ConfigLoader::new(gems);
        let hash = loader.load_file(&dir.join(".rubocop.yml")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(hash["AllCops"]["TargetRubyVersion"], Value::from(3.4));
        assert_eq!(hash["Style/Foo"]["Max"], Value::from(3));
        assert_eq!(hash["Style/Foo"]["Enabled"], Value::Bool(true));
        assert!(!hash.contains_key("inherit_from"));
    }
}
