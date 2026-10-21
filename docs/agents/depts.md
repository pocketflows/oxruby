# depts: Naming, Metrics, Security, Bundler, Gemspec, Migration

## Done (whole upstream spec passes; example counts)

Security: CompoundHash, Eval, IoMethods, JSONLoad, MarshalLoad, Open,
YAMLLoad (105 examples for the department; YAMLLoad's Ruby <= 3.0 examples
are `unsupported_on: :prism`).

Naming (besides ConstantName, MethodName, PredicatePrefix, VariableName):
BinaryOperatorParameterName 16, BlockParameterName 13,
ClassAndModuleCamelCase 5, HeredocDelimiterCase 25, HeredocDelimiterNaming 18,
MethodParameterName 23, RescuedExceptionsVariableName 37,
AccessorMethodName 25 (2 pending: they need the `rubydex` project index, as
upstream without it), MemoizedInstanceVariableName 74, BlockForwarding 35,
PredicateMethod 1262.

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


## Notes for whoever ports next

- Symbol-valued cop config in a spec's `cop_config` (`'Mode' => :conservative`)
  reaches the binary as the string `":conservative"` (YAML dump of a Symbol).
  Cops that `to_sym` a parameter should strip the leading `:`
  (Naming/PredicateMethod does).
- `Regexp` values (`'Regex' => /.../i`) arrive as `!ruby/regexp /.../i`
  tagged YAML values (see Naming/FileName on `depts-wip`).

## Mixins owned (claimed in `.coord/claims/`)

Landed: `heredoc` (`cop/mixin/heredoc.rs`: `HEREDOC_NODE_TYPES`,
`heredoc_node`, `indent_level`, `delimiter_string`, `heredoc_type`),
`uncommunicative_name`.

Claimed, not landed yet: `configurable_numbering`, `code_length`,
`method_complexity`, `gem_declaration`, `gemspec_help`, `ordered_gem_node`,
`ordered_gem_corrector`, `def_node`.

## Blocked

- Migration/DepartmentName: the spec expects `file.rb: Warning: no department
  given for Alias. Run ...` on stderr. That warning comes from
  `Registry#qualified_cop_name` while the comment directives are read
  (`comment_config.rs`, owned by `tokens`), and the shim
  (`spec/oxcop/binary.rb`, owned by `runtime`) drops the binary's stderr.
  The correction also needs every known cop name (not just ported ones) and
  `ConfigObsoletion.legacy_cop_names`.

- Naming/FileName (120/123, on `depts-wip`): the three "empty file" examples
  fail in the shim, not in the cop. A global offense (`add_global_offense`,
  `Offense::NO_LOCATION`) comes back from JSON as line 1, column 1, length 0;
  `spec/oxcop/cop_helper.rb#to_offense` builds a real `Parser::Source::Range`
  from it, and on an empty buffer `offense.location.source_line` raises
  ("One of the offenses has a misconstructed range"). **Ask for `runtime`:**
  map such offenses to `RuboCop::Cop::Offense::NO_LOCATION` (at least when
  the buffer is empty), as upstream's offenses are. FileName also needs
  `Config#allowed_camel_case_file?` in `config.rs` (patch on `depts-wip`).
- Naming/VariableNumber: its `offense`/`offense_array` shared examples assert
  `cop.config_to_allow_offenses`, which the Ruby cop fills from
  `correct_style_detected`/`unexpected_style_detected` during its own
  investigation; the shim never runs it. Needs plumbing from the binary
  (offense JSON or a side channel) into
  `Formatter::DisabledConfigFormatter.config_to_allow_offenses` (same for
  the 10 other specs that check it).

## Left

Naming: AsciiIdentifiers and InclusiveLanguage (need tokens), FileName and
VariableNumber (blocked, above).
Metrics: all (with `metrics/utils`). Bundler: all. Gemspec: all.
