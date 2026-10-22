//! Port of `lib/rubocop/config_loader.rb`, `config_loader_resolver.rb` and
//! `config_finder.rb`.
//!
//! Plugins (`plugins:`, and `require:` of a plugin gem) are integrated as in
//! `plugin/configuration_integrator.rb`, from the gems the Ruby launcher lists
//! in `OXCOP_GEMS` (see `Gems`).
//!
//! ERB in config files is rendered by Ruby (see `render_erb`).
//!
//! Not yet ported: remote `inherit_from` URLs.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use serde_yaml_ng::{Mapping, Value};

use crate::config::{Config, KnownCops};
use crate::{file_finder, path_util};

pub const DOTFILE: &str = ".rubocop.yml";
const XDG_CONFIG: &str = "config.yml";

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

/// Port of `ConfigFinder.find_config_path`: the nearest `.rubocop.yml` up
/// to the project root, then the project's `.config/`, then the user's.
/// `None` stands for `DEFAULT_FILE`.
pub fn find_config_path(target_dir: &Path) -> Option<PathBuf> {
    find_config_path_in(target_dir, project_root().as_deref())
}

fn find_config_path_in(target_dir: &Path, project_root: Option<&Path>) -> Option<PathBuf> {
    file_finder::find_file_upwards(DOTFILE, target_dir, project_root)
        .or_else(|| find_project_root_dot_config(project_root))
        .or_else(find_user_dotfile)
        .or_else(find_user_xdg_config)
}

/// `ConfigFinder.project_root`: the directory of the topmost `Gemfile` (or
/// `gems.rb`) above the working directory. No file search goes past it.
pub fn project_root() -> Option<PathBuf> {
    let pwd = path_util::pwd();
    let gems_file = file_finder::find_last_file_upwards("Gemfile", &pwd, None)
        .or_else(|| file_finder::find_last_file_upwards("gems.rb", &pwd, None))?;
    gems_file.parent().map(Path::to_path_buf)
}

fn find_project_root_dot_config(project_root: Option<&Path>) -> Option<PathBuf> {
    let project_root = project_root?;
    let dotfile = project_root.join(".config").join(DOTFILE);
    if dotfile.exists() {
        return Some(dotfile);
    }
    let xdg_config = project_root.join(".config/rubocop").join(XDG_CONFIG);
    xdg_config.exists().then_some(xdg_config)
}

fn find_user_dotfile() -> Option<PathBuf> {
    std::env::var_os("HOME")?;
    let file = std::env::home_dir()?.join(DOTFILE);
    file.exists().then_some(file)
}

fn find_user_xdg_config() -> Option<PathBuf> {
    let xdg_config_home = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) => path_util::expand_path(Path::new(&dir)),
        None => std::env::home_dir()?.join(".config"),
    };
    let xdg_config = xdg_config_home.join("rubocop").join(XDG_CONFIG);
    xdg_config.exists().then_some(xdg_config)
}

/// The gems the launcher (`gems/oxcop/lib/oxcop.rb`) found: under Bundler
/// the bundle's, otherwise the latest installed version of each. One line per
/// gem: `name<TAB>full_gem_path<TAB>default_lint_roller_plugin`.
#[derive(Debug, Default, Clone)]
pub struct Gems {
    /// `None` when the binary runs without the launcher.
    by_name: Option<HashMap<String, GemSpec>>,
    /// The launcher ran inside a bundle (`Bundler::SharedHelpers.in_bundle?`,
    /// `OXCOP_IN_BUNDLE`), so the gems are the bundle's.
    in_bundle: bool,
}

#[derive(Debug, Clone)]
struct GemSpec {
    dir: PathBuf,
    lint_roller_plugin: bool,
}

impl Gems {
    pub const ENV: &'static str = "OXCOP_GEMS";

    pub fn from_env() -> Self {
        let gems = std::env::var(Self::ENV)
            .map(|list| Self::parse(&list))
            .unwrap_or_default();
        Self {
            in_bundle: std::env::var_os("OXCOP_IN_BUNDLE").is_some_and(|v| v == "1"),
            ..gems
        }
    }

    /// The directories of the bundle's gems (`Bundler.load.specs`'
    /// `full_gem_path`), or `None` outside a bundle.
    pub fn bundle_dirs(&self) -> Option<Vec<PathBuf>> {
        if !self.in_bundle {
            return None;
        }
        let mut dirs: Vec<PathBuf> = self
            .by_name
            .as_ref()?
            .values()
            .map(|gem| gem.dir.clone())
            .collect();
        dirs.sort();
        Some(dirs)
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
            in_bundle: false,
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
    /// `ConfigLoader.enabled_by_default` (`--enable-all-cops`).
    pub enabled_by_default: Option<bool>,
    /// `ConfigLoader.disabled_by_default` (`--disable-all-cops`).
    pub disabled_by_default: Option<bool>,
    /// `ConfigLoader.preview` (`--[no-]preview`).
    pub preview: Option<bool>,
    /// `ConfigLoader.ignore_parent_exclusion` (`--ignore-parent-exclusion`).
    pub ignore_parent_exclusion: bool,
    /// `ConfigFinder.project_root`, found once per run.
    project_root: std::cell::OnceCell<Option<PathBuf>>,
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
            return Ok(Config::new(self.apply_default_overrides(&defaults), None));
        };
        let config_file = path_util::expand_path(config_file);
        let mut hash = self.load_file(&config_file)?;
        if !self.ignore_parent_exclusion {
            self.add_excludes_from_files(&mut hash, &config_file)?;
        }
        let merged = self.merge_with_default(hash, true);
        Ok(Config::new(merged, Some(config_file)))
    }

    /// The plugins integrated so far (`plugins:`, or a plugin gem in `require:`).
    pub fn loaded_plugins(&self) -> &HashSet<String> {
        &self.loaded_plugins
    }

    /// `ConfigFinder.project_root`, cached for the run.
    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.get_or_init(project_root).as_deref()
    }

    /// Port of `ConfigLoader.add_excludes_from_files`: the `AllCops: Exclude`
    /// of the topmost `.rubocop.yml` up to the project root applies too.
    fn add_excludes_from_files(
        &mut self,
        config: &mut Mapping,
        config_file: &Path,
    ) -> Result<(), ConfigError> {
        let project_root = self.project_root().map(Path::to_path_buf);
        let Some(exclusion_file) =
            file_finder::find_last_file_upwards(DOTFILE, config_file, project_root.as_deref())
        else {
            return Ok(());
        };
        if path_util::expand_path(&exclusion_file) == path_util::expand_path(config_file) {
            return Ok(());
        }
        let highest = self.load_file(&exclusion_file)?;
        add_excludes_from_higher_level(config, &highest, &exclusion_file);
        Ok(())
    }

    /// Port of `ConfigLoader.add_missing_namespaces`: cop names given
    /// without their department (or with the wrong one) are qualified, with
    /// `Registry.qualified_cop_name`'s warnings. The registry is every core
    /// cop plus the plugin cops in the default configuration. Not ported:
    /// skipping `ConfigObsoletion`'s deprecated names.
    fn add_missing_namespaces(
        &mut self,
        path: &Path,
        hash: &mut Mapping,
    ) -> Result<(), ConfigError> {
        let core = crate::config::core_cops();
        let mut registry = KnownCops::from_config(self.default_configuration());
        registry.extend(core);
        // Plugin and custom cops are registered by the time upstream gets
        // here (a plugin's own default.yml included); oxcop only knows them
        // from configuration keys, so take this file's keys outside the
        // core departments as registered.
        let own: Mapping = hash
            .keys()
            .filter(|key| {
                key.as_str()
                    .and_then(|k| k.rsplit_once('/'))
                    .is_some_and(|(dept, _)| {
                        !crate::directive_comment::CopRegistry::is_department(core, dept)
                    })
            })
            .map(|key| (key.clone(), Value::Null))
            .collect();
        registry.extend(&KnownCops::from_config(&own));
        let keys: Vec<String> = hash
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        for key in keys {
            let qualified = qualified_cop_name(&registry, &key, path)?;
            if qualified == key {
                continue;
            }
            if let Some(value) = hash.shift_remove(key.as_str()) {
                hash.insert(qualified.into(), value);
            }
        }
        Ok(())
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

        // Adding missing namespaces only after resolving requires & inheritance,
        // since both can introduce new cops that need to be considered here.
        self.add_missing_namespaces(path, &mut hash)?;

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
        // `base_configs`: glob entries are expanded with `Dir.glob`, which is
        // relative to the working directory.
        let inherited_files: Vec<String> = array(hash.get("inherit_from"))
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .flat_map(|f| {
                if path_util::glob(&f) {
                    path_util::dir_glob(&f)
                } else {
                    vec![f]
                }
            })
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
                        let mode = determine_inherit_mode(hash, key, self.preview_for(hash));
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

    /// Port of `ConfigLoaderResolver#merge_with_default`: if
    /// `AllCops:DisabledByDefault` is true, only cops from the user
    /// configuration are enabled; if `AllCops:EnabledByDefault` is true, only
    /// cops it disables are disabled. `--disable-all-cops` and
    /// `--enable-all-cops` take precedence over both.
    fn merge_with_default(&mut self, config: Mapping, unset_nil: bool) -> Mapping {
        let preview = self.preview_for(&config);
        let base_defaults = apply_preview_defaults(self.default_configuration(), preview);
        let (disabled_by_default, enabled_by_default) = self.resolve_default_overrides(&config);
        let mut config = config;
        let default_configuration = if disabled_by_default || enabled_by_default {
            let mut default_configuration =
                transform_enabled(&base_defaults, Value::Bool(!disabled_by_default));
            if disabled_by_default {
                config =
                    handle_disabled_by_default(&config, &mut default_configuration, &base_defaults);
            }
            default_configuration
        } else {
            base_defaults
        };
        override_enabled_for_disabled_departments(&default_configuration, &mut config);
        let mode = with_preview_exclude_merge(
            config
                .get("inherit_mode")
                .and_then(Value::as_mapping)
                .cloned()
                .unwrap_or_default(),
            preview,
        );
        merge_with(
            &default_configuration,
            &config,
            &MergeOptions {
                inherit_mode: Some(&mode),
                unset_nil,
            },
        )
    }

    /// `ConfigLoaderResolver#preview?`: `--[no-]preview`, else
    /// `AllCops: Preview` of the configuration being merged.
    fn preview_for(&self, config: &Mapping) -> bool {
        self.preview.unwrap_or_else(|| {
            config
                .get("AllCops")
                .and_then(Value::as_mapping)
                .and_then(|all_cops| all_cops.get("Preview"))
                == Some(&Value::Bool(true))
        })
    }

    /// `ConfigLoaderResolver#resolve_default_overrides`: the CLI options when
    /// either is given, else the configuration's `AllCops` settings (Ruby
    /// truthiness).
    fn resolve_default_overrides(&self, config: &Mapping) -> (bool, bool) {
        let (disabled, enabled) = (
            self.disabled_by_default.unwrap_or(false),
            self.enabled_by_default.unwrap_or(false),
        );
        if disabled || enabled {
            return (disabled, enabled);
        }
        let all_cops = config.get("AllCops").and_then(Value::as_mapping);
        let flag = |key| all_cops.and_then(|m| m.get(key)).is_some_and(truthy);
        (flag("DisabledByDefault"), flag("EnabledByDefault"))
    }

    /// Port of `ConfigLoader.apply_default_overrides`: the defaults as the
    /// whole configuration (no config file), with the same preview and
    /// `--enable-all-cops` / `--disable-all-cops` treatment
    /// `merge_with_default` gives them.
    fn apply_default_overrides(&self, defaults: &Mapping) -> Mapping {
        let hash = apply_preview_defaults(defaults, self.preview == Some(true));
        if self.enabled_by_default.is_none() && self.disabled_by_default.is_none() {
            return hash;
        }
        let disabled = self.disabled_by_default.unwrap_or(false);
        transform_enabled(&hash, Value::Bool(!disabled))
    }
}

/// `transform(config) { |params| params.merge('Enabled' => enabled) }` over
/// every top-level entry (`AllCops` and departments too, as upstream does).
fn transform_enabled(config: &Mapping, enabled: Value) -> Mapping {
    let mut result = config.clone();
    for (_, params) in result.iter_mut() {
        if let Some(params) = params.as_mapping_mut() {
            params.insert("Enabled".into(), enabled.clone());
        }
    }
    result
}

/// Port of `ConfigLoaderResolver#handle_disabled_by_default`. Cops of a
/// department the user configuration enables keep their original default
/// `Enabled`; every entry of the user configuration gets `Enabled: true`
/// unless it sets `Enabled` itself.
fn handle_disabled_by_default(
    config: &Mapping,
    new_default_configuration: &mut Mapping,
    base_defaults: &Mapping,
) -> Mapping {
    for (dept, dept_params) in config {
        let Some(dept) = dept.as_str().filter(|dept| !dept.contains('/')) else {
            continue;
        };
        let dept_enabled = dept_params
            .as_mapping()
            .and_then(|params| params.get("Enabled"))
            .is_some_and(truthy);
        if !dept_enabled {
            continue;
        }
        let prefix = format!("{dept}/");
        for (cop, params) in new_default_configuration.iter_mut() {
            let Some(cop) = cop.as_str().filter(|cop| cop.starts_with(&prefix)) else {
                continue;
            };
            let Some(params) = params.as_mapping_mut() else {
                continue;
            };
            // Retain original default configuration for cops in the department.
            let original = base_defaults
                .get(cop)
                .and_then(Value::as_mapping)
                .and_then(|base| base.get("Enabled"))
                .cloned()
                .unwrap_or(Value::Null);
            params.insert("Enabled".into(), original);
        }
    }

    // `{ 'Enabled' => true }.merge(params)`: set true if not set.
    let mut result = config.clone();
    for (_, params) in result.iter_mut() {
        let Some(mapping) = params.as_mapping() else {
            continue;
        };
        let mut with_enabled = Mapping::new();
        with_enabled.insert("Enabled".into(), Value::Bool(true));
        for (key, value) in mapping {
            with_enabled.insert(key.clone(), value.clone());
        }
        *params = Value::Mapping(with_enabled);
    }
    result
}

/// `ConfigLoaderResolver#with_preview_exclude_merge`: under `Preview`,
/// `Exclude` is merged rather than replaced, unless `inherit_mode` says
/// otherwise.
fn with_preview_exclude_merge(mut mode: Mapping, preview: bool) -> Mapping {
    if !preview {
        return mode;
    }
    let exclude = Value::String("Exclude".to_owned());
    if mode_includes(&mode, "override", &exclude) || mode_includes(&mode, "merge", &exclude) {
        return mode;
    }
    let mut merge = array(mode.get("merge"));
    merge.push(exclude);
    mode.insert("merge".into(), Value::Sequence(merge));
    mode
}

/// `Config#add_excludes_from_higher_level`.
fn add_excludes_from_higher_level(config: &mut Mapping, highest: &Mapping, highest_path: &Path) {
    let Some(Value::Sequence(highest_excludes)) = highest
        .get("AllCops")
        .and_then(Value::as_mapping)
        .and_then(|all_cops| all_cops.get("Exclude"))
    else {
        return;
    };
    let all_cops = config
        .entry("AllCops".into())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(all_cops) = all_cops.as_mapping_mut() else {
        return;
    };
    let excludes = all_cops
        .entry("Exclude".into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Some(excludes) = excludes.as_sequence_mut() else {
        return;
    };
    let highest_dir = highest_path.parent().unwrap_or(Path::new("."));
    for path in highest_excludes {
        let path = match path {
            // `!ruby/regexp` values stay as they are.
            Value::String(p) if !p.starts_with('/') => {
                Value::String(highest_dir.join(p).to_string_lossy().into_owned())
            }
            other => other.clone(),
        };
        if !excludes.contains(&path) {
            excludes.push(path);
        }
    }
}

/// `Registry#qualified_cop_name` (with `warn: true`), over `registry`.
fn qualified_cop_name(
    registry: &KnownCops,
    name: &str,
    path: &Path,
) -> Result<String, ConfigError> {
    // `Badge.parse`: the department is everything before the last `/`.
    let (department, cop_name) = match name.rsplit_once('/') {
        Some((department, cop_name)) => (Some(department), cop_name),
        None => (None, name),
    };
    let basename = |full: &str| full.rsplit('/').next().unwrap_or(full).to_owned();
    if department.is_none()
        && (name == "RedundantCopDisableDirective"
            || registry
                .cop_names()
                .iter()
                .any(|known| basename(known) == name))
    {
        emit_warning(path, &format!("no department given for {name}."));
    }
    if registry.contains(name) {
        return Ok(name.to_owned());
    }
    let mut departments: Vec<&str> = registry
        .cop_names()
        .iter()
        .filter_map(|known| known.rsplit_once('/').map(|(dept, _)| dept))
        .collect();
    departments.sort_unstable();
    departments.dedup();
    let potential: Vec<String> = departments
        .iter()
        .map(|dept| format!("{dept}/{cop_name}"))
        .filter(|candidate| registry.contains(candidate))
        .collect();
    match potential.as_slice() {
        [] => Ok(name.to_owned()),
        [real] => {
            // `resolve_badge`: a qualified name in the wrong department.
            if let Some(department) = department {
                let real_department = real.rsplit_once('/').map_or("", |(dept, _)| dept);
                if department != real_department {
                    emit_warning(
                        path,
                        &format!(
                            "{name} has the wrong namespace - replace it with \
                             {real_department}/{cop_name}"
                        ),
                    );
                }
            }
            Ok(real.clone())
        }
        _ => Err(ConfigError(format!(
            "Ambiguous cop name `{name}` used in {} needs department qualifier. Did you mean {}?",
            path.display(),
            potential.join(" or ")
        ))),
    }
}

/// `Registry#emit_warning`.
fn emit_warning(path: &Path, message: &str) {
    warn(&format!(
        "{}: Warning: {message}",
        path_util::smart_path(path)
    ));
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
fn determine_inherit_mode(hash: &Mapping, key: &Value, preview: bool) -> Mapping {
    let local = hash
        .get(key)
        .and_then(Value::as_mapping)
        .and_then(|cop| cop.get("inherit_mode"));
    let mode = local
        .or_else(|| hash.get("inherit_mode"))
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    with_preview_exclude_merge(mode, preview)
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
    /// `--enable-pending-cops` (`Some(true)`) / `--disable-pending-cops`.
    pub pending_cops_option: Option<bool>,
    /// `--preview` (`Some(true)`) / `--no-preview`.
    pub preview_option: Option<bool>,
}

impl ConfigStore {
    pub fn new(options_config: Option<PathBuf>) -> Self {
        Self {
            options_config: options_config.map(|p| path_util::expand_path(&p)),
            loader: ConfigLoader::new(Gems::from_env()),
            ..Default::default()
        }
    }

    /// The store a run with these options uses: `-c`, `--raw-config`, and
    /// the cop selection options `CLI#set_cop_selection_options_to_config_loader`
    /// hands `ConfigLoader` (and the registry).
    pub fn from_options(options: &crate::options::Options) -> Self {
        let mut store = Self::new(options.config.clone());
        store.raw = options.raw_config;
        store.pending_cops_option = options.pending_cops_option();
        store.preview_option = options.preview_option();
        store.loader.enabled_by_default = options.enable_all_cops.then_some(true);
        store.loader.disabled_by_default = options.disable_all_cops.then_some(true);
        store.loader.preview = options.preview_option();
        store.loader.ignore_parent_exclusion = options.ignore_parent_exclusion;
        store
    }

    pub fn for_file(&mut self, file: &Path) -> Result<Rc<Config>, ConfigError> {
        self.for_dir(file.parent().unwrap_or(Path::new(".")))
    }

    /// The gems the launcher found (`OXCOP_GEMS`).
    pub fn gems(&self) -> &Gems {
        &self.loader.gems
    }

    pub fn for_pwd(&mut self) -> Result<Rc<Config>, ConfigError> {
        self.for_dir(&path_util::pwd())
    }

    pub fn for_dir(&mut self, dir: &Path) -> Result<Rc<Config>, ConfigError> {
        let config_path = match &self.options_config {
            Some(path) => Some(path.clone()),
            None => {
                if !self.by_dir.contains_key(dir) {
                    let found = find_config_path_in(dir, self.loader.project_root());
                    self.by_dir.insert(dir.to_path_buf(), found);
                }
                self.by_dir[dir].clone()
            }
        };
        if !self.by_path.contains_key(&config_path) {
            let config = if self.raw
                && let Some(path) = &config_path
            {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
                let hash: Mapping = serde_yaml_ng::from_str(&contents)
                    .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
                let mut config = Config::new(hash, Some(path.clone()));
                // RuboCop's own specs load the plugin they test, so a plugin
                // counts as loaded when the configuration has its department
                // or one of its cops (rubocop-rails' `:config` context dumps
                // only `AllCops` and the cop under test).
                for (department, plugin) in crate::cop::base::PLUGIN_DEPARTMENTS {
                    let prefix = format!("{department}/");
                    if config.get(department).is_some()
                        || config
                            .to_h()
                            .keys()
                            .any(|key| key.as_str().is_some_and(|key| key.starts_with(&prefix)))
                    {
                        config.loaded_plugins.insert((*plugin).to_owned());
                    }
                }
                config
            } else {
                let mut config = self
                    .loader
                    .configuration_from_file(config_path.as_deref())?;
                config.loaded_plugins = self.loader.loaded_plugins().clone();
                config
            };
            let mut config = config;
            config.pending_cops_option = self.pending_cops_option;
            config.preview_option = self.preview_option;
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

    /// A scratch directory holding `files`, for one test.
    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxcop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, contents) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        dir
    }

    /// `configuration_from_file(file_path)` in config_loader_spec.rb.
    fn configuration(dir: &Path, loader: &mut ConfigLoader) -> Config {
        loader
            .configuration_from_file(Some(&dir.join(".rubocop.yml")))
            .unwrap()
    }

    /// `config.for_cop(cop)['Enabled']`: upstream's `for_cop` stores
    /// `enable_cop?`, whose truthiness `cop_enabled_in_config` is.
    fn enabled(config: &Config, cop: &str) -> bool {
        config.cop_enabled_in_config(cop)
    }

    // config_loader_spec.rb: "when a parent file specifies DisabledByDefault: true"
    #[test]
    fn disabled_by_default_in_a_parent_file_disables_cops() {
        let dir = project(
            "dbd-parent",
            &[
                ("disable.yml", "AllCops:\n  DisabledByDefault: true\n"),
                (".rubocop.yml", "inherit_from: disable.yml\n"),
            ],
        );
        let config = configuration(&dir, &mut ConfigLoader::default());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(config.for_cop("Style/Alias")["Enabled"], Value::Bool(false));
    }

    // config_loader_spec.rb: "when a department is disabled", the shared
    // examples with a nested `Foo/Bar` department.
    #[test]
    fn department_disabling_resolves_for_all_cops() {
        for (enabled_by_default, disabled_by_default) in
            [(false, false), (false, true), (true, false)]
        {
            let child = format!(
                "inherit_from: parent_rubocop.yml\n\nAllCops:\n  EnabledByDefault: \
                 {enabled_by_default}\n  DisabledByDefault: {disabled_by_default}\n\nLayout:\n  \
                 Enabled: false\n\nLayout/LineLength:\n  Enabled: true\n\nStyle:\n  Enabled: \
                 false\n\nMetrics/MethodLength:\n  Enabled: true\n\nMetrics/ClassLength:\n  \
                 Enabled: false\n\nLint/RaiseException:\n  Enabled: true\n\nStyle/AndOr:\n  \
                 Enabled: true\n"
            );
            let dir = project(
                "dept-disabled",
                &[
                    (
                        "grandparent_rubocop.yml",
                        "Layout:\n  Enabled: false\n\nLayout/EndOfLine:\n  Enabled: true\n\n\
                         Naming/FileName:\n  Enabled: pending\n\nMetrics/AbcSize:\n  Enabled: \
                         true\n\nMetrics/PerceivedComplexity:\n  Enabled: true\n\nLint:\n  \
                         Enabled: false\n\nFoo/Bar/Baz:\n  Enabled: true\n",
                    ),
                    (
                        "parent_rubocop.yml",
                        "inherit_from: grandparent_rubocop.yml\n\nMetrics:\n  Enabled: false\n\n\
                         Metrics/AbcSize:\n  Enabled: false\n\nNaming:\n  Enabled: false\n\n\
                         Foo/Bar:\n  Enabled: false\n",
                    ),
                    (".rubocop.yml", &child),
                ],
            );
            let config = configuration(&dir, &mut ConfigLoader::default());
            std::fs::remove_dir_all(&dir).unwrap();
            let case = format!(
                "EnabledByDefault: {enabled_by_default}, DisabledByDefault: {disabled_by_default}"
            );
            for (cop, expected) in [
                ("Layout/DotPosition", false),
                ("Layout/EndOfLine", false),
                ("Layout/LineLength", true),
                ("Metrics/MethodLength", true),
                ("Metrics/ClassLength", false),
                ("Metrics/AbcSize", false),
                ("Metrics/PerceivedComplexity", false),
                ("Naming/FileName", false),
                ("Style/Alias", false),
                ("Style/AndOr", true),
                ("Lint/RaiseException", true),
                ("Foo/Bar/Baz", false),
                ("Foo/BarBaz", !disabled_by_default),
                ("Lint/StructNewOverride", false),
                ("Bundler/DuplicatedGem", !disabled_by_default),
            ] {
                assert_eq!(enabled(&config, cop), expected, "{cop} with {case}");
            }
        }
    }

    // config_loader_spec.rb: "EnabledByDefault / DisabledByDefault"
    #[test]
    fn disabled_by_default_enables_the_cops_the_config_mentions() {
        let dir = project(
            "dbd-mentioned",
            &[(
                ".rubocop.yml",
                "AllCops:\n  DisabledByDefault: true\nStyle/Copyright:\n  Exclude:\n  - foo\n",
            )],
        );
        let config = configuration(&dir, &mut ConfigLoader::default());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            config.for_cop("Style/Copyright")["Enabled"],
            Value::Bool(true)
        );
        assert_eq!(
            config.for_cop("Layout/TrailingWhitespace")["Enabled"],
            Value::Bool(false)
        );
    }

    #[test]
    fn disabled_by_default_with_an_enabled_department_keeps_its_defaults() {
        let dir = project(
            "dbd-dept",
            &[(
                ".rubocop.yml",
                "AllCops:\n  DisabledByDefault: true\nStyle:\n  Enabled: true\n",
            )],
        );
        let config = configuration(&dir, &mut ConfigLoader::default());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(enabled(&config, "Style/Alias"));
        assert!(!enabled(&config, "Layout/HashAlignment"));
        assert!(!enabled(&config, "Style/AutoResourceCleanup"));
    }

    #[test]
    fn enabled_by_default_enables_cops_unless_the_config_disables_them() {
        let dir = project(
            "ebd",
            &[(
                ".rubocop.yml",
                "AllCops:\n  EnabledByDefault: true\nLayout/TrailingWhitespace:\n  Enabled: false\n",
            )],
        );
        let config = configuration(&dir, &mut ConfigLoader::default());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(enabled(&config, "Layout/FirstMethodArgumentLineBreak"));
        assert!(!enabled(&config, "Layout/TrailingWhitespace"));
    }

    #[test]
    fn enable_all_cops_and_disable_all_cops_override_the_config() {
        for (option, file, cop, expected) in [
            (true, "", "Style/CollectionMethods", true),
            (
                true,
                "AllCops:\n  DisabledByDefault: true\n",
                "Style/CollectionMethods",
                true,
            ),
            (false, "", "Layout/TrailingWhitespace", false),
            (
                false,
                "AllCops:\n  EnabledByDefault: true\n",
                "Style/CollectionMethods",
                false,
            ),
        ] {
            let dir = project("all-cops", &[(".rubocop.yml", file)]);
            let mut loader = ConfigLoader::default();
            if option {
                loader.enabled_by_default = Some(true);
            } else {
                loader.disabled_by_default = Some(true);
            }
            let config = configuration(&dir, &mut loader);
            std::fs::remove_dir_all(&dir).unwrap();
            assert_eq!(enabled(&config, cop), expected, "{cop} with {file:?}");
        }
        // No config file: `apply_default_overrides`.
        let mut loader = ConfigLoader {
            disabled_by_default: Some(true),
            ..Default::default()
        };
        let config = loader.configuration_from_file(None).unwrap();
        assert!(!enabled(&config, "Layout/TrailingWhitespace"));
        assert!(enabled(&config, "Lint/Syntax"));
    }

    // config_loader_spec.rb: "when a new cop is introduced"
    #[test]
    fn a_pending_cop_follows_disabled_and_enabled_by_default() {
        for (file, expected) in [
            ("", Value::from("pending")),
            (
                "Metrics/MethodLength:\n  Enabled: true\n",
                Value::Bool(true),
            ),
            (
                "Metrics/MethodLength:\n  Enabled: false\n",
                Value::Bool(false),
            ),
            ("AllCops:\n  DisabledByDefault: true\n", Value::Bool(false)),
            ("AllCops:\n  EnabledByDefault: true\n", Value::Bool(true)),
        ] {
            let dir = project("new-cop", &[(".rubocop.yml", file)]);
            let mut loader = ConfigLoader {
                default_configuration: Some(yaml(
                    "AllCops:\n  AnythingGoes: banana\nMetrics/MethodLength:\n  Enabled: pending\n",
                )),
                ..Default::default()
            };
            let config = configuration(&dir, &mut loader);
            std::fs::remove_dir_all(&dir).unwrap();
            assert_eq!(
                config.for_cop("Metrics/MethodLength")["Enabled"],
                expected,
                "{file:?}"
            );
        }
    }

    // config_loader_spec.rb: "when a department is configured without an
    // Enable value specified"
    #[test]
    fn a_department_without_enabled_still_enables_its_configured_cop() {
        for (name, files) in [
            (
                "inline",
                vec![(
                    ".rubocop.yml",
                    "AllCops:\n  DisabledByDefault: true\n\nCustom:\n  Foo: Bar\n\nCustom/Cop:\n  \
                     Enabled: true\n",
                )],
            ),
            (
                "inherit",
                vec![
                    ("third_party/default.yml", "Custom:\n  Foo: Bar\n"),
                    (
                        ".rubocop.yml",
                        "inherit_from:\n  - 'third_party/default.yml'\n\nAllCops:\n  \
                         DisabledByDefault: true\n\nCustom/Cop:\n  Enabled: true\n",
                    ),
                ],
            ),
        ] {
            let dir = project(&format!("dept-no-enabled-{name}"), &files);
            let config = configuration(&dir, &mut ConfigLoader::default());
            std::fs::remove_dir_all(&dir).unwrap();
            assert!(enabled(&config, "Custom/Cop"), "{name}");
        }
    }

    // A plugin's cops are disabled under DisabledByDefault unless the user
    // configuration mentions the cop or enables its department.
    #[test]
    fn disabled_by_default_applies_to_plugin_cops() {
        let dir = project(
            "dbd-plugin",
            &[
                (
                    "rubocop-fake/config/default.yml",
                    "Fake:\n  Enabled: true\nFake/On:\n  Enabled: true\nFake/Off:\n  Enabled: \
                     false\nFake/Configured:\n  Enabled: true\n  Max: 1\n",
                ),
                (
                    ".rubocop.yml",
                    "plugins: [rubocop-fake]\nAllCops:\n  DisabledByDefault: true\n\
                     Fake/Configured:\n  Max: 2\nStyle/HashSyntax:\n  EnforcedShorthandSyntax: always\n",
                ),
            ],
        );
        let gems = Gems::parse(&format!(
            "rubocop-fake\t{}\tFake::Plugin\n",
            dir.join("rubocop-fake").display()
        ));
        let config = configuration(&dir, &mut ConfigLoader::new(gems));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!enabled(&config, "Fake/On"));
        assert!(!enabled(&config, "Fake/Off"));
        assert!(enabled(&config, "Fake/Configured"));
        assert_eq!(config.for_cop("Fake/Configured")["Max"], Value::from(2));
        assert!(enabled(&config, "Style/HashSyntax"));
        assert!(!enabled(&config, "Style/Alias"));
    }

    // config_loader_spec.rb: ".configuration_file_for"
    #[test]
    fn config_files_are_found_up_to_the_project_root_then_in_its_dot_config() {
        let dir = project(
            "finder",
            &[
                ("outside/.rubocop.yml", ""),
                ("outside/app/Gemfile", ""),
                ("outside/app/dir/example.rb", ""),
            ],
        );
        let root = dir.join("outside/app");
        // A spurious config outside of the project is ignored.
        let found = find_config_path_in(&root.join("dir"), Some(&root));
        assert_ne!(found, Some(dir.join("outside/.rubocop.yml")));
        std::fs::create_dir_all(root.join(".config/rubocop")).unwrap();
        std::fs::write(root.join(".config/rubocop/config.yml"), "").unwrap();
        assert_eq!(
            find_config_path_in(&root.join("dir"), Some(&root)),
            Some(root.join(".config/rubocop/config.yml"))
        );
        std::fs::write(root.join(".config/.rubocop.yml"), "").unwrap();
        assert_eq!(
            find_config_path_in(&root.join("dir"), Some(&root)),
            Some(root.join(".config/.rubocop.yml"))
        );
        std::fs::write(root.join("dir/.rubocop.yml"), "").unwrap();
        assert_eq!(
            find_config_path_in(&root.join("dir"), Some(&root)),
            Some(root.join("dir/.rubocop.yml"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // config_loader_spec.rb: "when multiple config files exist in ancestor
    // directories" and "ignores parent AllCops/Exclude if
    // ignore_parent_exclusion is true".
    #[test]
    fn all_cops_exclude_comes_from_the_highest_directory_level_too() {
        let dir = project(
            "parent-exclusion",
            &[
                (".rubocop.yml", "AllCops:\n  Exclude:\n    - vendor/**\n"),
                ("dir/.rubocop.yml", "AllCops:\n  Exclude: []\n"),
                ("vendor/.rubocop.yml", "AllCops:\n  Exclude:\n    - 'foo'\n"),
            ],
        );
        let loader = || {
            let loader = ConfigLoader::default();
            loader.project_root.set(Some(dir.clone())).unwrap();
            loader
        };
        let excludes = |config: &Config| config.for_all_cops()["Exclude"].clone();
        let config = loader()
            .configuration_from_file(Some(&dir.join("dir/.rubocop.yml")))
            .unwrap();
        let vendor = Value::from(dir.join("vendor/**").to_string_lossy().into_owned());
        assert_eq!(excludes(&config), Value::Sequence(vec![vendor.clone()]));

        let mut ignoring = loader();
        ignoring.ignore_parent_exclusion = true;
        let config = ignoring
            .configuration_from_file(Some(&dir.join("vendor/.rubocop.yml")))
            .unwrap();
        let excludes = excludes(&config);
        let excludes = excludes.as_sequence().unwrap();
        assert!(!excludes.contains(&vendor));
        assert!(excludes.contains(&Value::from(
            dir.join("vendor/foo").to_string_lossy().into_owned()
        )));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // `ConfigLoader.add_missing_namespaces`
    #[test]
    fn cop_names_without_their_department_are_qualified() {
        let dir = project(
            "namespaces",
            &[(
                ".rubocop.yml",
                "LineLength:\n  Max: 100\nStyle/IndentationWidth:\n  Width: 4\n\
                 Rails/SafeNavigation:\n  Enabled: true\nStyle/PreferSomething:\n  Enabled: true\n",
            )],
        );
        let hash = ConfigLoader::default()
            .load_file(&dir.join(".rubocop.yml"))
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let keys: Vec<&str> = hash.keys().filter_map(Value::as_str).collect();
        assert_eq!(
            keys,
            [
                "Rails/SafeNavigation",
                "Style/PreferSomething",
                "Layout/LineLength",
                "Layout/IndentationWidth"
            ]
        );
        assert_eq!(hash["Layout/LineLength"]["Max"], Value::from(100));
        let registry = crate::config::core_cops();
        let error = qualified_cop_name(
            &{
                let mut known = registry.clone();
                let extra: Mapping = yaml("Foo/LineLength: {}");
                known.extend(&KnownCops::from_config(&extra));
                known
            },
            "LineLength",
            Path::new(".rubocop.yml"),
        )
        .unwrap_err();
        assert_eq!(
            error.0,
            "Ambiguous cop name `LineLength` used in .rubocop.yml needs department qualifier. \
             Did you mean Foo/LineLength or Layout/LineLength?"
        );
    }

    // `base_configs`: `inherit_from` globs go through `Dir.glob`.
    #[test]
    fn inherit_from_globs_are_expanded() {
        let dir = project(
            "inherit-glob",
            &[
                ("configs/a.yml", "Style/Foo:\n  Max: 1\n"),
                (
                    "configs/deep/b.yml",
                    "Style/Foo:\n  Max: 2\nStyle/Bar:\n  Max: 3\n",
                ),
                ("configs/.hidden.yml", "Style/Baz:\n  Max: 4\n"),
                ("x.yml", "Style/Qux:\n  Max: 5\n"),
            ],
        );
        let config = format!(
            "inherit_from:\n  - '{}/configs/**/*.yml'\n  - '{}/{{x,y}}.yml'\n",
            dir.display(),
            dir.display()
        );
        std::fs::write(dir.join(".rubocop.yml"), config).unwrap();
        assert_eq!(
            path_util::dir_glob(&format!("{}/configs/**/*.yml", dir.display())),
            [
                format!("{}/configs/a.yml", dir.display()),
                format!("{}/configs/deep/b.yml", dir.display())
            ]
        );
        let hash = ConfigLoader::default()
            .load_file(&dir.join(".rubocop.yml"))
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // Later files win: deep/b.yml after a.yml.
        assert_eq!(hash["Style/Foo"]["Max"], Value::from(2));
        assert_eq!(hash["Style/Bar"]["Max"], Value::from(3));
        assert_eq!(hash["Style/Qux"]["Max"], Value::from(5));
        assert!(!hash.contains_key("Style/Baz"));
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
