//! Port of `lib/rubocop/config_loader.rb`, `config_loader_resolver.rb` and
//! `config_finder.rb`.
//!
//! Not yet ported: `inherit_gem`, remote `inherit_from` URLs, `inherit_mode`,
//! `DisabledByDefault` handling of user-enabled cops, and ERB in config files.

use std::cell::RefCell;
use std::collections::HashMap;
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

/// Port of `ConfigLoader.configuration_from_file`.
pub fn configuration_from_file(config_file: Option<&Path>) -> Result<Config, ConfigError> {
    let Some(config_file) = config_file else {
        let mut config = Config::new(merge_with_default(Mapping::new()), None);
        config.make_excludes_absolute();
        return Ok(config);
    };
    let config_file = path_util::expand_path(config_file);
    let hash = load_file(&config_file)?;
    let mut config = Config::new(merge_with_default(hash), Some(config_file));
    config.make_excludes_absolute();
    Ok(config)
}

/// Port of `ConfigLoader.load_file`: reads one file and resolves its
/// `inherit_from` chain. `Exclude` patterns are made absolute relative to the
/// file that declared them.
pub fn load_file(path: &Path) -> Result<Mapping, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        ConfigError(format!(
            "Configuration file not found: {} ({e})",
            path.display()
        ))
    })?;
    let value: Value = serde_yaml_ng::from_str(&contents)
        .map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
    let mut hash = match value {
        Value::Mapping(mapping) => mapping,
        Value::Null => Mapping::new(),
        _ => return Err(ConfigError(format!("{} is not a hash", path.display()))),
    };

    warn_about_unsupported_keys(&hash);

    let mut own = Config::new(hash.clone(), Some(path.to_path_buf()));
    own.make_excludes_absolute();
    hash = own.to_h().clone();

    let base_dir = path.parent().unwrap_or(Path::new("."));
    let inherited = match hash.remove("inherit_from") {
        Some(Value::String(file)) => vec![file],
        Some(Value::Sequence(files)) => files
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    let mut result = Mapping::new();
    for file in inherited {
        if file.starts_with("http://") || file.starts_with("https://") {
            warn_once(&format!(
                "remote inherit_from ({file}) is not supported yet; skipping"
            ));
            continue;
        }
        let inherited_hash = load_file(&base_dir.join(&file))?;
        result = merge(&result, &inherited_hash);
    }
    Ok(merge(&result, &hash))
}

/// Port of `ConfigLoaderResolver#merge_with_default`.
fn merge_with_default(mut config: Mapping) -> Mapping {
    let mut defaults = apply_preview_defaults(default_configuration(), false);
    let all_cops = config.get("AllCops").and_then(Value::as_mapping);
    let flag = |key| {
        all_cops
            .and_then(|m| m.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
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
    merge(&defaults, &config)
}

fn apply_preview_defaults(defaults: &Mapping, preview: bool) -> Mapping {
    let mut result = defaults.clone();
    for (_, params) in result.iter_mut() {
        let Some(params) = params.as_mapping_mut() else {
            continue;
        };
        if let Some(Value::Mapping(preview_params)) = params.remove("Preview")
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

/// Port of `ConfigLoaderResolver#merge`: a recursive hash merge.
pub fn merge(base: &Mapping, derived: &Mapping) -> Mapping {
    let mut result = base.clone();
    for (key, derived_value) in derived {
        let merged = match (base.get(key), derived_value) {
            (Some(Value::Mapping(b)), Value::Mapping(d)) => Value::Mapping(merge(b, d)),
            _ => derived_value.clone(),
        };
        result.insert(key.clone(), merged);
    }
    result
}

fn warn_about_unsupported_keys(hash: &Mapping) {
    for key in ["require", "plugins"] {
        if let Some(value) = hash.get(key) {
            let names = match value {
                Value::String(s) => s.clone(),
                Value::Sequence(items) => items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(s) => Some(s.clone()),
                        Value::Mapping(m) => {
                            m.keys().next().and_then(Value::as_str).map(str::to_owned)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => continue,
            };
            warn_once(&format!(
                "Ruby extensions in `{key}` are not supported and will be ignored: {names}"
            ));
        }
    }
    if hash.contains_key("inherit_gem") {
        warn_once("`inherit_gem` is not supported yet and will be ignored");
    }
}

fn warn_once(message: &str) {
    thread_local! {
        static SEEN: RefCell<std::collections::HashSet<String>> = RefCell::default();
    }
    SEEN.with(|seen| {
        if seen.borrow_mut().insert(message.to_owned()) {
            eprintln!("oxcop: {message}");
        }
    });
}

/// Port of `ConfigStore`: caches the resolved configuration per config file
/// and finds the right one for each inspected file.
#[derive(Default)]
pub struct ConfigStore {
    options_config: Option<PathBuf>,
    by_path: HashMap<Option<PathBuf>, Rc<Config>>,
    by_dir: HashMap<PathBuf, Option<PathBuf>>,
}

impl ConfigStore {
    pub fn new(options_config: Option<PathBuf>) -> Self {
        Self {
            options_config: options_config.map(|p| path_util::expand_path(&p)),
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
            let config = configuration_from_file(config_path.as_deref())?;
            self.by_path.insert(config_path.clone(), Rc::new(config));
        }
        Ok(Rc::clone(&self.by_path[&config_path]))
    }
}
