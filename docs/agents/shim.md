# shim: spec shim features and the cops blocked on them

Owner: `shim`. Owns `spec/oxcop/`, `spec/spec_helper.rb`,
`crates/oxcop/src/server.rs` and the auto-gen-state handling in
`runner.rs` (`runtime` owns the rest of `runner.rs`, `cli.rs`, the config
modules and `script/land`). runtime landed most of the binary channel
before handing over (9fe3cd9, d98b527: `--prior`, `--auto-gen-state`,
`--keep-stdin-path`, `--lsp`, real-file inspection, NO_LOCATION, cop
errors, warnings to `$stderr`); see `runtime.md`.

## Shim features added here

- **Stubbed `Dir.pwd`**: `oxcop --server` takes a request cwd that doesn't
  exist (a spec's `allow(Dir).to receive(:pwd)`) as PathUtil's `Dir.pwd`
  (`path_util::set_pwd_override`) instead of failing. Server only; a
  spawned process (`OXCOP_NO_SERVER`, changed ENV) can't do this.
- **`--default-external ENC`** (hidden): the shim passes
  `Encoding.default_internal || default_external` when it isn't UTF-8;
  `util::ruby_inspect` then escapes every non-ASCII character, as Ruby's
  `String#inspect` does (`with_default_external_encoding`).
- **Offense run with autocorrect**: `expect_offense` sets the cop's
  `@options[:autocorrect]`, so when it is set the offense run passes
  `-A --autocorrect-once` (not for real files): `autocorrect?` is true
  while investigating, as upstream.
- **Invalid strings in the cop config** are dumped as binary (`!binary`).
- `Investigation::add_offense_yield` / `offense_yields`: an `add_offense`
  block that updates the cop (`self.max =`, style detection) runs when
  RuboCop would yield (new offense, enabled line).
- Mixins: EndKeywordAlignment detects styles; CodeLength and
  MethodComplexity set exclude limits, use the LSP location, and an unknown
  `CountAsOne` raises `RuboCop::Warning`; `format_g4` is a port of Ruby's
  `BSD__dtoa` mode 2 (Ruby rounds 107.45 to 107.4 and 131.05 to "131.0";
  checked against Ruby on 28,000 values); LineLengthHelp `valid_uri?` is
  `URI.parse` (uri 1.1.1 RFC 3986 parser plus the LDAP/FTP/mailto scheme
  checks), and inside Layout/LineLength `max_line_length` is the cop's own
  `Max` (it aliases it) even when the configuration disables the cop.

## Landed (whole spec passing; examples)

From `lint-a-wip`: Lint/PercentStringArray 22, PercentSymbolArray 12,
DuplicateMethods 532 (5 pending: project index).
From `layout-a-wip`: Layout/EndOfLine 17, BeginEndAlignment 7,
DefEndAlignment 18, EndAlignment 216, EmptyLineAfterMagicComment 30.
From `style-b-wip`: Style/FormatStringToken 420.
From `style-d-wip`: Style/WordArray 60.
From `depts-wip`: Bundler/GemFilename 15, Gemspec/RequiredRubyVersion 21,
Naming/InclusiveLanguage 37, FileName 123, VariableNumber 78,
Metrics/AbcSize 25, BlockLength 38, BlockNesting 26, ClassLength 34,
CyclomaticComplexity 37, ModuleLength 21, ParameterLists 17.
From `layout-b-wip`: Layout/RedundantLineBreak 114, LineLength 209,
SpaceBeforeBlockBraces 18, SpaceInsideBlockBraces 43.
From `style-a-wip`: Style/Copyright 25. From `style-c-wip`:
Style/RedundantArgument 15.

## Left

- Lint/ScriptPermission (not ported; the shim inspects real files now).
- Migration/DepartmentName (not ported; runtime's d98b527 forwards the
  "no department given" warning).
- Style/DocumentationMethod (not ported; needs the `def_node` mixin).

## Principled failures

None so far: every landed cop passes its whole spec.
