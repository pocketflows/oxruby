# depts: Naming, Metrics, Security, Bundler, Gemspec, Migration

Every cop in these departments is ported. The ones not on `main` are green
except for examples that need harness support owned by `runtime` (the
requests below) or the token stream (`tokens`). Branches:

- `depts`: everything green is landed on main.
- `depts-wip` (local): cops blocked on the requests below, each passing
  every other example of its spec. Rebase it onto main, and land a cop as
  soon as its request is done.

## Requests for `runtime` (shim / CLI)

1. **`:lsp` examples** (Metrics/ClassLength, ModuleLength, BlockLength,
   CyclomaticComplexity, AbcSize: one example each). The spec turns on
   `RuboCop::LSP.enable`; `CodeLength`/`MethodComplexity` then report only
   the first line (`node.source_range.begin.join(loc.name || loc.begin)`).
   Needs the shim to pass e.g. a hidden `--lsp` flag when
   `RuboCop::LSP.enabled?`, and `CopOptions`/`Investigation` to expose it.
2. **Cop errors raised in specs** (`expect { ... }.to raise_error(RuboCop::Warning)`:
   Metrics/BlockLength's invalid `CountAsOne`; also Style/Copyright,
   Style/StringLiterals, Layout/EndOfLine and others). RuboCop's specs run
   with `raise_cop_error`, so an exception in a cop reaches the example. The
   binary would need to report the error class and message (cops push to
   `Investigation::errors`; `code_length::count_as_one` returns the message)
   and the shim re-raise it as that class.
3. **Global offenses on an empty file** (Naming/FileName 3 examples,
   Naming/InclusiveLanguage 3, Gemspec/RequiredRubyVersion 1). A global
   offense (`add_global_offense`, `Offense::NO_LOCATION`) comes back from
   JSON as line 1, column 1, length 0; `spec/oxcop/cop_helper.rb#to_offense`
   builds a real `Parser::Source::Range` from it, and on an empty buffer
   `offense.location.source_line` raises ("One of the offenses has a
   misconstructed range"). Map such offenses to
   `RuboCop::Cop::Offense::NO_LOCATION` (at least when the buffer is empty),
   as upstream's are.
4. **`config_to_allow_offenses`** (Naming/VariableNumber 29 of 78, and 10
   other specs). The Ruby cop fills it from `correct_style_detected` /
   `unexpected_style_detected` during its own investigation; the shim never
   runs it. Needs plumbing from the binary (offense JSON or a side channel)
   into `Formatter::DisabledConfigFormatter.config_to_allow_offenses`.
5. **`read_exclude_limit(cop)`** (Metrics/BlockNesting 3 examples,
   Metrics/ParameterLists 1; also Layout/LineLength, Style/NumericLiterals,
   ...). Upstream's `exclude_limit` setters (`self.max = n`) append `"n\n"`
   to `ExcludeLimit.tmp_dir/<Dept-Cop>/<Param>` (the spec context sets
   `tmp_dir` to a temp directory) and the spec reads the files back. The shim
   could pass the directory (e.g. a hidden `--exclude-limit-dir DIR` when
   `RuboCop::ExcludeLimit.tmp_dir` is set) and the binary write the same
   files; `Investigation` would get an `exclude_limit(param, value)` that
   cops call where upstream sets `self.max = ...` (only for enabled
   offenses, as it runs in the `add_offense` block).
6. **The investigated file's name** (Bundler/GemFilename, 8 of 15). The spec
   stubs `processed_source.buffer.name` (`'gems.rb'`, `'spec/gems.rb'`) and
   calls `_investigate` itself; the shim sends `processed_source.path`
   (`nil`, so `(string)`), and the binary expands the `--stdin` path to an
   absolute one, while the messages quote the name as given
   (`(file path: spec/gems.rb)`). Needs the shim to use `buffer.name` and the
   binary to keep the given name for `ProcessedSource#file_path` there.
7. **Migration/DepartmentName** (not ported). Its spec expects
   `file.rb: Warning: no department given for Alias. Run ...` on stderr.
   Upstream prints that from `Registry#qualified_cop_name` while reading the
   comment directives (`comment_config.rs`, owned by `tokens`), and the shim
   drops the binary's stderr. The correction also needs every known cop name
   (not only ported ones) and `ConfigObsoletion.legacy_cop_names`.

`FileName` also needs `Config#allowed_camel_case_file?` in `config.rs`
(`runtime`); the patch is in the FileName commit on `depts-wip`.

## Done (whole upstream spec passes; example counts)

Security: CompoundHash, Eval, IoMethods, JSONLoad, MarshalLoad, Open,
YAMLLoad (105 for the department; YAMLLoad's Ruby <= 3.0 examples are
`unsupported_on: :prism`).

Naming (besides ConstantName, MethodName, PredicatePrefix, VariableName):
AccessorMethodName 25 (2 pending: they need the `rubydex` project index, as
upstream without it), BinaryOperatorParameterName 16, BlockForwarding 35,
BlockParameterName 13, ClassAndModuleCamelCase 5, HeredocDelimiterCase 25,
HeredocDelimiterNaming 18, MemoizedInstanceVariableName 74,
AsciiIdentifiers 12,
MethodParameterName 23, PredicateMethod 1262, RescuedExceptionsVariableName 37.

Metrics: CollectionLiteralLength 13, MethodLength 33, PerceivedComplexity 36.

Bundler: DuplicatedGem 10, DuplicatedGroup 21, GemComment 27, GemVersion 6,
InsecureProtocolSource 6, OrderedGems 18.

Gemspec: AddRuntimeDependency 5, AttributeAssignment 7, DependencyVersion 77,
DeprecatedAttributeAssignment 18, DevelopmentDependencies 14,
DuplicatedAssignment 19, OrderedDependencies 18, RequireMFA 16,
RubyVersionGlobalsUsage 5.

`Include` patterns from default.yml work for Bundler and Gemspec: in a
directory with `Gemfile`, `gems.rb`, `foo.gemspec` and `.rb` files with the
same content, Bundler cops only report on `Gemfile`/`gems.rb` and Gemspec
cops only on `foo.gemspec`.

## Blocked (on `depts-wip`)

| Cop | Passing | Blocked by |
| --- | --- | --- |
| Metrics/AbcSize | 24/25 | request 1 |
| Metrics/BlockLength | 36/38 | requests 1, 2 |
| Metrics/BlockNesting | 23/26 | request 5 |
| Metrics/ClassLength | 33/34 | request 1 |
| Metrics/CyclomaticComplexity | 36/37 | request 1 |
| Metrics/ModuleLength | 20/21 | request 1 |
| Metrics/ParameterLists | 16/17 | request 5 |
| Naming/FileName | 120/123 | request 3 (+ `config.rs` patch) |
| Naming/VariableNumber | 49/78 | request 4 |
| Bundler/GemFilename | 7/15 | request 6 |
| Gemspec/RequiredRubyVersion | 20/21 | request 3 |
| Naming/InclusiveLanguage | 34/37 | request 3 |
| Migration/DepartmentName | not ported | request 7 |

## Shared changes

- `ast/node_pattern.rs`: node types written with a dash (`op-asgn`) are
  `op_asgn`; a node type inside a variadic union at a sequence head
  (`({def | defs _} ...)`) matches the bare type; `^` at a sequence head
  ascends from the node itself (`(^^(send ...) _ :hash)`).
- `cop/mixin/mod.rs`: `regex!`, `ruby_regex` and node-pattern regexps go
  through `ruby_pattern`, which spells `\w \W \d \D \s \S` as ASCII classes.
  Ruby's Onigmo treats them as ASCII-only while Oniguruma's are Unicode;
  POSIX brackets (`[[:word:]]`, `[[:upper:]]`) and `\b` stay Unicode, as in
  Ruby. (Oniguruma's `*_IS_ASCII` options would make the brackets ASCII too.)
  `magic_comment.rs` (owned by `tokens`) builds its regexps directly.
  Also `config_regexp`, `regexp_literal`, `regexp_to_s` for `Regexp` config
  values.
- `cop/mixin/allowed_methods.rs`, `allowed_pattern.rs`: `Regexp` values
  (`!ruby/regexp`) in `AllowedPatterns`/`IgnoredPatterns`, and the deprecated
  `IgnoredMethods`/`ExcludedMethods` holding a `Regexp` (they then count as
  patterns, not method names), as upstream.
- `build.rs`: a department subdirectory with a `mod.rs` becomes a public
  module of the department (`cop::metrics::utils`).

## Mixins owned (claimed in `.coord/claims/`)

`heredoc` (`HEREDOC_NODE_TYPES`, `heredoc_node`, `indent_level`,
`delimiter_string`, `heredoc_type`), `uncommunicative_name`, `code_length`
(`check_code_length(cx, node, label)`), `method_complexity`
(`method_to_check`, `check_method_complexity`, `complexity`, `format_g4` for
`%.4g`), `gem_declaration`, `gemspec_help`, `ordered_gem_node`,
`correctors/ordered_gem_corrector`, `metrics/utils/` (CodeLengthCalculator,
AbcSizeCalculator, IteratingBlock, RepeatedAttributeDiscount,
RepeatedCsendDiscount), `configurable_numbering` (on `depts-wip`, with
VariableNumber). Released: `def_node` (GemComment includes it but uses
nothing from it).

## Notes for whoever ports next

- Symbol-valued cop config in a spec's `cop_config` (`'Mode' => :conservative`)
  reaches the binary as the string `":conservative"` (YAML dump of a Symbol).
  Cops that `to_sym` a parameter should strip the leading `:`
  (Naming/PredicateMethod, Bundler/GemFilename do).
- `Regexp` values (`'Regex' => /.../i`) arrive as `!ruby/regexp /.../i`
  tagged YAML values; use `mixin::config_regexp`.
- `exclude_limit` / `self.max =` and `LSP.enabled?` aren't ported (requests
  1 and 5).
