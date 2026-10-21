# depts: Naming, Metrics, Security, Bundler, Gemspec, Migration

## Done (whole upstream spec passes; example counts)

Security: CompoundHash, Eval, IoMethods, JSONLoad, MarshalLoad, Open,
YAMLLoad (105 examples for the department; YAMLLoad's Ruby <= 3.0 examples
are `unsupported_on: :prism`).

Naming (besides ConstantName, MethodName, PredicatePrefix, VariableName):
BinaryOperatorParameterName 16, BlockParameterName 13,
ClassAndModuleCamelCase 5, HeredocDelimiterCase 25, HeredocDelimiterNaming 18,
MethodParameterName 23.

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

## Left

Naming: AccessorMethodName, AsciiIdentifiers, BlockForwarding, FileName,
InclusiveLanguage, MemoizedInstanceVariableName, PredicateMethod,
RescuedExceptionsVariableName, VariableNumber.
Metrics: all (with `metrics/utils`). Bundler: all. Gemspec: all.
