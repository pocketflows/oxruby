# shim: spec shim features and the cops blocked on them

Owner: `shim`. Owns `spec/oxcop/`, `spec/spec_helper.rb`,
`crates/oxcop/src/server.rs` and the auto-gen-state handling in
`runner.rs` (`runtime` owns the rest of `runner.rs`, `cli.rs`, the config
modules and `script/land`).

## Landed

- Lint/PercentStringArray 22, PercentSymbolArray 12 (from `lint-a-wip`),
  Layout/EndOfLine 17 (from `layout-a-wip`): their non-UTF-8 examples pass
  since runtime's 3fcfe14 (magic encoding comments).

## In progress

See the blocker list in the task: auto-gen state across calls, `:lsp`,
`read_exclude_limit`, global offenses on empty files, file names, cop
instances across files, real file paths, the "no department given" warning.
