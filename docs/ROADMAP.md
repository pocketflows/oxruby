# Roadmap

## oxcop: status (handoff notes)

Branches (git worktree `.worktrees/oxcop-cops`):

- `oxcop-cops` is green: `cargo build`, `cargo test` and `bundle exec rake`
  pass. It still parses through **`vendor/ruby-prism`**, a copy of
  ruby-prism 1.9.0 with an added FFI function (`parse_with_options`, with a
  lexer callback; see `vendor/ruby-prism/OXCOP.md`). The user has since ruled
  that out: no `unsafe` in any code we write, vendored patches included.
- `oxcop-cops-prism2-wip` (one commit on top) removes the vendored crate and
  uses crates.io `ruby-prism = "=2.0.0-rc.2"` (pinned in the root
  `[workspace.dependencies]`; the stree branch uses the same pin). It builds,
  the AST translation is adapted (2.0 renames, e.g. `UnlessNode#unless_keyword_loc`,
  `SymbolNode#content_loc`, `InNode#in_keyword_loc`, new `NoBlockParameterNode`
  and `ErrorRecoveryNode`) and `script/compare_ast` is still identical on
  `../code` and rubocop's `lib/` and `spec/`. **Not finished:** 2.0 has no
  lexer API, so `processed_source.tokens` is an empty stub
  (`ast/token_builder.rs`). The ported specs still pass, but token-based
  behaviour (comment-directive line classification in `CommentConfig`,
  `FrozenStringLiteral#leading_comment_lines`) is wrong. **Next step:**
  write the safe token builder (below), then make the 2.0 switch the next
  commit on `oxcop-cops`.

### Done

Infrastructure (`crates/oxcop/src/`; every file says what it ports):

- `ast/`: Prism tree → the `parser` gem's AST (`compiler.rs`, `builder.rs`),
  a pre-order arena with parent links, rubocop-ast node helpers (`node.rs`,
  one `Node` handle; `==` is identity, `same_as` is structural), the
  NodePattern interpreter (`node_pattern.rs`; patterns are copied verbatim),
  the comment associator, and the parser-gem token lexer port (`lexer.rs`,
  `Prism::Translation::Parser::Lexer`, fed from Prism's token stream).
  `script/compare_ast [--tokens] DIR...` diffs AST/tokens against RuboCop's.
- `cop/`: `Cop` trait (`on_node`/`after_node` per node type,
  `restrict_on_send`), Commissioner walk, Team (correcting cops first,
  `autocorrect_incompatible_with`), Corrector (port of `TreeRewriter` with
  RuboCop's clobbering policies), `mixin/` (RangeHelp, Util,
  ConfigurableEnforcedStyle, AllowedMethods/Pattern, FrozenStringLiteral,
  CommentsHelp, VisibilityHelp, NegativeConditional, SafeAssignment,
  Parentheses, ConfigurableNaming, IgnoredNode), `correctors/`
  (ParenthesesCorrector). `build.rs` generates the registry from the files in
  each department directory (a cop file defines a `Default` struct named
  after the file in CamelCase).
- `comment_config.rs`, `directive_comment.rs`: `rubocop:disable/enable/todo`,
  `disable all`, departments, push/pop, `*-next`, and the 1.91 opt-in of
  config-disabled cops through `enable` comments. `magic_comment.rs`.
- `ProcessedSource` normalizes `\r\n` like `Parser::Source::Buffer`
  (`raw_source` vs `buffer_source`). `Config#for_badge` merges department
  config into cop config.
- Spec shim (`spec/oxcop/`): sends `cop.config` with `--raw-config` (RuboCop's
  specs use `Config.new(hash)` without `default.yml`), `--persist-cops` (the
  `expect_correction` loop keeps one cop instance, which some specs rely on),
  `--autocorrect-once` for `loop: false`, and skips `unsupported_on: :prism`
  examples as upstream does under Prism. `spec/support/project_index_metadata.rb`
  was missing from the vendored support files and was copied verbatim.

Cops ported, each passing its whole upstream spec file (example counts):
Layout/LeadingCommentSpace 33, Layout/TrailingWhitespace 19,
Lint/AssignmentInCondition 75, Lint/EnsureReturn 8, Lint/Syntax (spec drives
the Commissioner directly; 8 examples never reach oxcop),
Naming/ConstantName 24, Naming/MethodName 237, Naming/PredicatePrefix 23
(2 pending: they need the `rubydex` project index, which isn't installed, the
same as upstream without it), Naming/VariableName 118,
Style/ClassMethodsDefinitions 17, Style/IfUnlessModifierOfIfUnless 7,
Style/InvertibleUnlessCondition 17, Style/MutableConstant 246,
Style/NegatedUnless 14, Style/NestedModifier 14, Style/OrAssignment 22,
Style/ParenthesesAroundCondition 32, Style/UnlessElse 5. That is every core
cop `../code` enables.

Scoreboard (`bundle exec rspec`): 18 cops ported, 919 examples, 0 failures,
2 pending; 28,618 examples skipped for 595 unported cops.
`script/compare_with_rubocop ../code`: 0 mismatched files; 9 files (`bin/*`)
inspected only by oxcop because rubocop-rails's `AllCops: Exclude: bin/*`
isn't loaded (see plugin default configs below). rubocop 9.3s, oxcop 1.1s.

### Left, in priority order

1. **Safe token builder** (on `oxcop-cops-prism2-wip`). Build the stream
   `Prism.lex` would give (type, byte range, plus `EXPR_BEG|EXPR_LABEL` for
   hash/hash-pattern `{`), then reuse `lexer.rs`. Plan, calibrated against
   `Prism.lex`:
   - Walk the Prism tree and emit tokens from the locations it records
     (`keyword_loc`, `operator_loc`, `opening/closing_loc`, `message_loc`,
     `name_loc`, `call_operator_loc`, `delimiter_loc`, literals, variables).
     The node tells you the type: `IfNode` modifier → `KEYWORD_IF_MODIFIER`,
     loop `do` → `KEYWORD_DO_LOOP`, `SplatNode` → `USTAR`, `BlockArgumentNode`
     → `UAMPERSAND`, `ArrayNode` `[` → `BRACKET_LEFT_ARRAY`, index `[` →
     `BRACKET_LEFT`, `-1` → `UMINUS_NUM` + `INTEGER`, `foo (1)` →
     `PARENTHESIS_LEFT_PARENTHESES`, `a:` → `LABEL`, `"a":` → `STRING_BEGIN`,
     `STRING_CONTENT`, `LABEL_END`, `:foo` → `SYMBOL_BEGIN` + `IDENTIFIER`,
     method names after `.` are `IDENTIFIER` even when spelled like keywords,
     names ending in `?`/`!` are `METHOD_NAME`.
   - Strings: `STRING_BEGIN`/`CONTENT`/`END`, `EMBEXPR_BEGIN/END`, `EMBVAR`;
     `%w` elements separated by `WORDS_SEP`; squiggly heredocs get one
     `STRING_CONTENT` per line, other heredocs one for the body; heredoc body
     tokens come right after their `HEREDOC_START`, then the rest of the line
     (`HEREDOC_END` includes the newline).
   - Comments come from the parse result. An inline `COMMENT` token includes
     its trailing `\n`, and no newline token follows it. `=begin` gives
     `EMBDOC_BEGIN`/`EMBDOC_LINE`/`EMBDOC_END`; `data_loc` gives `__END__`.
   - Scan the gaps between tokens. They should only hold whitespace, `,`,
     `;`, `\`-newline continuations and newlines. A newline is `NEWLINE`
     when the previous token leaves the lexer in an END/ARG/CMDARG/ENDFN/MID
     state (identifiers, literals, `)`, `]`, `}`, `end`, `return`, `->`,
     def names, ...), and `IGNORED_NEWLINE` after BEG/CLASS/FNAME/DOT states
     (operators, `,`, `(`, `[`, `{`, `|`, `do`, `then`, `else`, `def`, `.`,
     ...), after a `NEWLINE`, or when the next code line starts with `.`/`&.`.
     Gap text that is none of these means a token was missed: assert in tests.
   - Validate with a `Prism.lex` dumper against `--dump-prism-tokens` on
     `../code` and rubocop's `lib/` and `spec/`, then with
     `script/compare_ast --tokens`. Record anything that can't be covered here.
2. Plugin default configs: have the launcher (`gems/oxcop/lib/oxcop.rb`)
   resolve `plugins:` gems' `config/default.yml` and pass them to the binary.
   That fixes the `bin/*` diff. Then ERB in `.rubocop.yml`, `inherit_mode`
   merge/override and `inherit_gem`. Make `script/compare_with_rubocop` run
   the launcher inside `../code`'s bundle.
3. Port the rest of the cops, department by department. Shared helpers go in
   `cop/mixin/` and `cop/correctors/`. Only register a cop when its whole
   spec passes and actually invokes the binary. Heavily used mixins still to
   port: Alignment, SurroundingSpace, PercentLiteral, Heredoc,
   CheckAssignment, StatementModifier, EndKeywordAlignment, TrailingComma,
   and the multiline/first-element families.
4. `Lint/RedundantCopDisableDirective` (it runs after the inspection loop on
   all offenses, disabled ones included; `CommentConfig` already keeps the
   ranges and directives), the pending-cops warning, the remaining CLI options,
   the result cache, and parallel inspection with rayon (already a
   dependency).
5. The full spec run makes two binary calls per example (~80 ms in a debug
   build). Run spec files in parallel, or use a release binary
   (`OXCOP_BINARY`).

### Known gaps and decisions

- No `unsafe`: `#![forbid(unsafe_code)]` in `lib.rs`/`main.rs`. Crates.io
  dependencies may use `unsafe` internally; our code and vendored patches may
  not.
- RuboCop parses with the `parser` gem below Ruby 3.3 and with Prism from 3.3
  on. We always use Prism, with the target version from `TargetRubyVersion`
  (3.3 at least). `Lint/Syntax` messages are Prism's.
- `parser` (Ruby < 3.3) unescapes `\\` in squiggly heredocs differently from
  Prism (8 files in rubocop's own specs).
- Token `text` for strings that aren't valid UTF-8, and for rational and
  imaginary literals, differs from RuboCop.
- `IgnoredNodes` matches by range, as Ruby's does across the spec loop.
- The project index (`rubydex`) isn't supported; cops behave as RuboCop does
  without one.
- Remaining `TargetRuby` sources (`Gemfile.lock`, `*.gemspec`,
  `RUBOCOP_TARGET_RUBY_VERSION`), hidden-file and `RubyInterpreters` edge
  cases in `TargetFinder`.

## Formatter: syntax_tree drop-in (second gem)

Goal: replace `../code`'s `bin/rails stree_new:{write,check}`
(`lib/tasks/stree_new.rake`) with a Rust binary:

- syntax_tree 6.3.0 printing (print width and plugins from `.streerc`) for
  `.rb`, `Gemfile`, `.rake`, ...
- `.erb` via w_syntax_tree-erb 0.12.0 semantics.
- `<script>`/`<style>` bodies and whole `.js/.css/.json.erb` files formatted by
  oxfmt (see `lib/tasks/support/erb_embedded_formatter.rb` in `../code`).
- `.streerc` `--ignore-files` handling, git-based file discovery, and
  `*:modified` variants that diff against the merge-base with `main`.
- `write` and `check` modes with the same messages and exit codes.

Clone syntax_tree (and w_syntax_tree-erb) as siblings when this starts.
Parsing, file discovery and shared infrastructure should move into a shared
crate at that point.
