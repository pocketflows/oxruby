# Compiled-in plugins: an app's own cops in Rust

RuboCop loads an application's custom cops at run time (`require:
./lib/rubocop/cop/custom/foo` in `.rubocop.yml`). oxcop can't load Ruby, so an
application ports its cops to Rust and builds its own `oxcop` binary with them
compiled in, the way esbuild plugins work: a small crate depends on the
`oxcop` library, registers its cops, and has a `main` that runs the usual
command line. Everything else (configuration, directives, formatters, the
gem launcher) is the stock `oxcop`.

## The API

```rust
// oxcop (the library)
pub struct Oxcop;                       // the command line, with plugins
impl Oxcop {
    pub fn new() -> Self;
    pub fn plugin(self, plugin: plugin::Plugin) -> Self;
    pub fn run(self) -> std::process::ExitCode;          // std::env::args_os()
    pub fn run_with_args(self, args: impl IntoIterator<Item = OsString>) -> u8;
    pub fn install(self) -> Result<(), plugin::PluginError>; // register only (tests)
}

// oxcop::plugin
pub struct Plugin;
impl Plugin {
    pub fn new(name: impl Into<String>) -> Self;
    pub fn default_config(self, yaml: &'static str) -> Self;
    pub fn provides_require(self, feature: impl Into<String>) -> Self;
    pub fn ignores_require(self, feature: impl Into<String>) -> Self;
    pub fn cop<C: Cop + Default + 'static>(self) -> Self;
}
pub fn check(plugins: Vec<Plugin>) -> Result<Installed, PluginError>;

// oxcop::prelude: Cop, Investigation, Node, NodeType, Child, NodePattern,
// Corrector, Range, ProcessedSource, Config, Offense, Severity, raise,
// CopError, regex!, path_util, Mapping, Value, and the mixin modules
// (mixin, util, range_help, allowed_methods, allowed_pattern,
// configurable_enforced_style, ignored_node).
```

What each piece does:

- **Cops** join the registry (`cop::registry::all()`/`names()`) before
  anything reads it, so everything sees them like built-in cops: team
  mobilization, `--only`/`--except`, department directives
  (`# rubocop:disable Custom`), `rubocop:disable all`, the hidden
  `--show-ported-cops`/`--show-resolved-cops`, and the offenses formatter's
  `[Safe Correctable]` labels. A cop may live in a built-in department
  (`Style/Foo`). A name already used by a built-in cop or another plugin's
  cop, a name without a department, or a default configuration that isn't a
  YAML mapping makes `run()` exit 2 with `Error: ...`.
- **`default_config`** is the plugin's `config/default.yml`. It is
  integrated into the default configuration exactly as RuboCop integrates a
  lint_roller plugin's (`Plugin::ConfigurationIntegrator`): `.rubocop.yml`
  overrides it, `DisabledByDefault` applies to it, and its keys make the
  cops known to directives and to `Lint/RedundantCopDisableDirective`.
  Compiled-in plugins are always loaded; no `plugins:` entry is needed (one
  naming the plugin is satisfied silently). The YAML is plain: no ERB,
  `inherit_from`, `require` or `plugins`. Relative `Include`/`Exclude`
  patterns are relative to the working directory, as for a gem plugin's
  default.yml.
- **`provides_require(feature)`** claims a `require:` entry, exactly as
  written in `.rubocop.yml` (`./lib/rubocop/cop/custom/foo`): the plugin's
  cops are its port, so the entry is satisfied silently. Unclaimed entries
  still warn `Ruby extensions in require are not supported and will be
  ignored`.
- **`ignores_require(feature)`** claims an entry the binary deliberately
  skips because it does nothing a lint run needs (an `at_exit` hook, a patch
  for another tool). It silences the warning like `provides_require`; the
  separate name records the intent.

## A minimal plugin

```
my_app/oxcop/
  Cargo.toml
  config/default.yml
  src/lib.rs
  src/main.rs
  src/cop/custom/no_sleep.rs     # one file per cop, mirroring lib/rubocop/cop/
```

`Cargo.toml`:

```toml
[package]
name = "my-app-oxcop"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
publish = false

[[bin]]
name = "oxcop"
path = "src/main.rs"

[dependencies]
oxcop = { path = "../../oxruby/crates/oxcop" }
# or: oxcop = { git = "https://github.com/pocketflows/oxruby", rev = "..." }

[workspace]          # its own workspace, so its profiles apply

[profile.release]
lto = "thin"
strip = true
```

`src/main.rs`:

```rust
#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    oxcop::Oxcop::new().plugin(my_app_oxcop::plugin()).run()
}
```

`src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub mod cop {
    pub mod custom {
        mod no_sleep;
        pub use no_sleep::NoSleep;
    }
}

pub fn plugin() -> oxcop::plugin::Plugin {
    oxcop::plugin::Plugin::new("my-app-cops")
        .default_config(include_str!("../config/default.yml"))
        .provides_require("./lib/rubocop/cop/custom/no_sleep")
        .cop::<cop::custom::NoSleep>()
}
```

`src/cop/custom/no_sleep.rs`, the port of a Ruby cop with
`RESTRICT_ON_SEND = %i[sleep]` and `def on_send(node)`:

```rust
//! Port of `lib/rubocop/cop/custom/no_sleep.rb`.

use oxcop::prelude::*;

const MSG: &str = "Do not `sleep` in %<where>s.";

#[derive(Default)]
pub struct NoSleep;

impl Cop for NoSleep {
    fn name(&self) -> &'static str {
        "Custom/NoSleep"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["sleep"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_some() {
            return;
        }
        let place = cx.cop_config_str("Where").unwrap_or("app code");
        cx.add_offense(node.source_range(), &MSG.replace("%<where>s", place));
    }
}
```

`config/default.yml` holds what the cop implies by itself (its
`Description`, defaults its code falls back to); the app's `.rubocop.yml`
keeps `Enabled`, `Include` and the parameters it already sets.

Build with `cargo build --release --manifest-path oxcop/Cargo.toml`, then run
the result through the gem launcher so it gets the Ruby-side environment
(`OXCOP_GEMS` for gem plugins, ERB rendering):
`OXCOP_BINARY=oxcop/target/release/oxcop bundle exec oxcop`. A `bin/oxcop`
wrapper that does both is a few lines of shell.

## Requirements for the plugin crate

- **Edition 2024, MSRV 1.88**, like oxcop.
- **`#![forbid(unsafe_code)]`** at the crate roots is recommended; oxcop
  forbids it everywhere.
- **No `panic = "abort"`** in any profile. A cop that panics is reported as a
  cop error (`An error occurred while Custom/Foo cop was inspecting ...`) and
  the run goes on; that needs unwinding (`catch_unwind`). With abort, one
  bad cop kills the whole run.
- **A path or git dependency.** oxcop isn't on crates.io (it
  `include_str!`s RuboCop's `default.yml` from `../../vendor` and depends on
  `rubydex` from git), so depend on a checkout by path, or on the repository
  by git at a `rev`. Copying oxruby's `Cargo.lock` into the crate keeps
  dependency versions the same as oxcop's tested ones.
- **Set your own release profile**, and an empty `[workspace]` table so the
  crate isn't taken for part of an enclosing workspace. oxruby's own
  profile (`lto = "fat"`, one codegen unit) doesn't apply to dependents.

## Porting a cop

- Each Rust file ports one Ruby file and starts with `//! Port of <path>`.
  Keep method names and their order where practical; copy messages
  verbatim.
- Callbacks: `on_send`/`on_csend`/`on_block`/... become `node_types()` plus
  `on_node` (dispatch on `node.ty()` when there are several);
  `after_send` and friends are `after_node_types()`/`after_node`.
  `RESTRICT_ON_SEND` is `restrict_on_send()`. `on_new_investigation` and
  `on_investigation_end` exist as is.
- `extend AutoCorrector` is `support_autocorrect() -> true`, with
  `cx.add_offense_with_correction(range, msg, |corrector| ...)`. The
  offenses formatter labels such cops `[Safe Correctable]` (or `Unsafe`
  when the default configuration says `SafeAutoCorrect: false`).
- `def_node_matcher`/`def_node_search` patterns port as `static X:
  NodePattern = NodePattern::new("...")` with `X.is_match(node)` (and
  captures; see the built-in cops for examples).
- `cop_config['Foo']` is `cx.cop_config_value("Foo")` (or `_str`, `_bool`,
  `_int`, `_list`); structured parameters (lists of hashes) are YAML
  `Value`s.
- `processed_source.file_path` is `cx.processed_source.file_path()`;
  `Pathname.pwd`/`Dir.pwd` is `path_util::pwd()` (not
  `std::env::current_dir`: the spec server answers for the spec's working
  directory); `File.fnmatch?` is `path_util::fnmatch`, RuboCop's
  `match_path?` is `path_util::match_path`.
- Ruby regexp literals are `regex!(r"...")` (Onigmo syntax, compiled once).
- Positions are byte offsets internally; reported columns are characters.
- A Ruby `raise` in a cop is `raise("ArgumentError", "message")`.

## Specs

`script/plugin_specs <plugin crate dir> [spec files...]` builds the crate's
debug binary and runs the app's own cop specs (the usual `RSpec.describe
RuboCop::Cop::Custom::Foo, :config` with `expect_offense`) through oxcop's
spec shim against it, from this repository. The specs load the app's Ruby cop
file for the described class and its configuration; the investigation itself
goes to the binary (`--stdin`, `--only`, the dumped `let(:config)`). rspec
and the binary share the working directory, so cops that check paths
relative to `Dir.pwd` behave the same in both. Without spec files it runs
every spec under `<crate>/../spec/lib/rubocop/cop/` that uses
`rubocop/rspec/support`; others (a hand-built Commissioner, a booted Rails
app) are skipped.

For a cop with no spec, the oracle is the Ruby cop itself run by real
RuboCop. Do that in a scratch directory outside the app (its `.rubocop.yml`
may install hooks): write fixture files and a scratch `.rubocop.yml` enabling
just that cop with the app's parameters, then compare
`BUNDLE_GEMFILE=<oxruby>/Gemfile bundle exec rubocop --require
<app>/lib/rubocop/cop/custom/foo.rb --only Custom/Foo --format emacs .` with
the plugin binary's output on the same directory.

## Limitations

- Plugin cops in the `Rails/` or `RSpec/` departments only run when that
  gem plugin is loaded (`Config::cop_registered`), like the built-in ports.
- The offenses formatter shows no progress bar on a terminal.
- A compiled plugin's default configuration is plain YAML (see above).
