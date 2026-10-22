//! Compiled-in plugins: an application's own cops, written in Rust against
//! this crate and linked into the application's own `oxcop` binary, the way
//! esbuild plugins are. This plays the part of RuboCop's `require:` of a cop
//! file plus a lint_roller plugin's `config/default.yml`. See
//! `docs/PLUGINS.md`.
//!
//! ```no_run
//! # use oxcop::prelude::*;
//! # #[derive(Default)]
//! # struct DisallowedMethods;
//! # impl Cop for DisallowedMethods {
//! #     fn name(&self) -> &'static str { "Custom/DisallowedMethods" }
//! # }
//! fn main() -> std::process::ExitCode {
//!     let plugin = oxcop::plugin::Plugin::new("my-app-cops")
//!         .default_config("Custom/DisallowedMethods:\n  Description: No.\n")
//!         .provides_require("./lib/rubocop/cop/custom/disallowed_methods")
//!         .cop::<DisallowedMethods>();
//!     oxcop::Oxcop::new().plugin(plugin).run()
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde_yaml_ng::{Mapping, Value};

use crate::cop::Cop;
use crate::cop::registry::CopConstructor;

/// A set of cops compiled into the binary, with their default
/// configuration and the `require:` entries they stand in for.
pub struct Plugin {
    name: String,
    default_config: Option<&'static str>,
    requires: Vec<(String, RequireClaim)>,
    cops: Vec<CopConstructor>,
}

/// What a plugin says about a `require:` entry of a configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequireClaim {
    /// The plugin's cops are the Rust port of what the feature defines.
    Provided,
    /// The feature does nothing a linter run needs, so the binary skips it
    /// on purpose.
    Ignored,
}

impl Plugin {
    /// A plugin named `name`, like a lint_roller plugin's gem name. A
    /// `plugins:` entry with this name is satisfied by the compiled plugin.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_config: None,
            requires: Vec::new(),
            cops: Vec::new(),
        }
    }

    /// The plugin's `config/default.yml`, usually `include_str!`ed. It is
    /// integrated into the default configuration as RuboCop integrates a
    /// lint_roller plugin's (`Plugin::ConfigurationIntegrator`), so it is the
    /// base that `.rubocop.yml` files override. Relative `Include`/`Exclude`
    /// patterns are relative to the working directory, as for a plugin's
    /// default.yml. It is plain YAML: no ERB, `inherit_from`, `require` or
    /// `plugins`.
    pub fn default_config(mut self, yaml: &'static str) -> Self {
        self.default_config = Some(yaml);
        self
    }

    /// Claims a `require:` entry, exactly as written in a configuration file
    /// (`./lib/rubocop/cop/custom/foo`): the plugin's cops are the port of
    /// what it defines. Claimed entries are satisfied silently; others still
    /// warn that Ruby extensions are not supported.
    pub fn provides_require(mut self, feature: impl Into<String>) -> Self {
        self.requires.push((feature.into(), RequireClaim::Provided));
        self
    }

    /// Claims a `require:` entry the binary deliberately does not port,
    /// because it does nothing a linter run needs: an `at_exit` hook, a
    /// monkey patch for another tool, a formatter you don't use. Like
    /// [`Plugin::provides_require`], it silences the warning; the difference
    /// is the stated intent, so a reader of `plugin()` can tell a port from a
    /// skip. For example, a hook that regenerates a report when the real
    /// `rubocop` exits has nothing to do in `oxcop`.
    pub fn ignores_require(mut self, feature: impl Into<String>) -> Self {
        self.requires.push((feature.into(), RequireClaim::Ignored));
        self
    }

    /// Adds a cop. Its name (`Department/Name`) must not be taken by a
    /// built-in cop or another plugin's. Plugin cops may live in a built-in
    /// department (`Style/Foo`).
    pub fn cop<C: Cop + Default + 'static>(mut self) -> Self {
        self.cops.push(construct::<C>);
        self
    }

    /// The plugin's name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn construct<C: Cop + Default + 'static>() -> Box<dyn Cop> {
    Box::new(C::default())
}

/// Why a set of plugins can't be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError(pub String);

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PluginError {}

/// The plugins of this process, installed once before anything reads the
/// cop registry.
#[derive(Default)]
pub struct Installed {
    /// Each plugin's name and parsed default configuration, in order.
    pub(crate) plugins: Vec<(String, Option<Mapping>)>,
    pub(crate) cops: Vec<CopConstructor>,
    pub(crate) cop_names: Vec<&'static str>,
    requires: HashMap<String, RequireClaim>,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// The installed plugins. Reading them before [`install`] freezes the
/// process with no plugins, and a later `install` fails.
pub(crate) fn installed() -> &'static Installed {
    INSTALLED.get_or_init(Installed::default)
}

/// The names of the installed plugins.
pub fn plugin_names() -> impl Iterator<Item = &'static str> {
    installed().plugins.iter().map(|(name, _)| name.as_str())
}

/// Whether an installed plugin claims this `require:` entry.
pub fn require_claim(feature: &str) -> Option<RequireClaim> {
    installed().requires.get(feature).copied()
}

/// Checks `plugins` without installing them: every cop name is
/// `Department/Name` and unique among the built-in cops and all plugins'
/// cops, plugin names are unique, and each default configuration is a YAML
/// mapping.
pub fn check(plugins: Vec<Plugin>) -> Result<Installed, PluginError> {
    let builtin: HashSet<&'static str> = crate::cop::CONSTRUCTORS
        .iter()
        .map(|new| new().name())
        .collect();
    let mut installed = Installed::default();
    let mut owners: HashMap<&'static str, String> = HashMap::new();
    let mut plugin_names = HashSet::new();
    for plugin in plugins {
        if !plugin_names.insert(plugin.name.clone()) {
            return Err(PluginError(format!(
                "plugin `{}` is registered twice",
                plugin.name
            )));
        }
        for new in &plugin.cops {
            let name = new().name();
            if !name.contains('/') {
                return Err(PluginError(format!(
                    "cop `{name}` of plugin `{}` needs a department (`Department/{name}`)",
                    plugin.name
                )));
            }
            if builtin.contains(name) {
                return Err(PluginError(format!(
                    "cop `{name}` of plugin `{}` has the name of a built-in cop",
                    plugin.name
                )));
            }
            if let Some(owner) = owners.get(name) {
                return Err(PluginError(format!(
                    "cop `{name}` of plugin `{}` is already registered by plugin `{owner}`",
                    plugin.name
                )));
            }
            owners.insert(name, plugin.name.clone());
            installed.cops.push(*new);
            installed.cop_names.push(name);
        }
        let default_config = plugin
            .default_config
            .map(|yaml| parse_default_config(&plugin.name, yaml))
            .transpose()?;
        for (feature, claim) in plugin.requires {
            installed.requires.insert(feature, claim);
        }
        installed.plugins.push((plugin.name, default_config));
    }
    Ok(installed)
}

fn parse_default_config(plugin: &str, yaml: &str) -> Result<Mapping, PluginError> {
    let error = |e: &dyn std::fmt::Display| {
        PluginError(format!("default configuration of plugin `{plugin}`: {e}"))
    };
    match serde_yaml_ng::from_str::<Value>(yaml).map_err(|e| error(&e))? {
        Value::Mapping(mapping) => Ok(mapping),
        Value::Null => Ok(Mapping::new()),
        _ => Err(error(&"not a YAML mapping")),
    }
}

/// Checks and installs `plugins` for this process. It must run before
/// anything reads the cop registry or the configuration, and only once.
pub fn install(plugins: Vec<Plugin>) -> Result<(), PluginError> {
    let installed = check(plugins)?;
    INSTALLED.set(installed).map_err(|_| {
        PluginError(
            "plugins must be installed once, before the cop registry is first used".to_owned(),
        )
    })
}
