# tokens

Owner of the safe token builder (`ast/token_builder.rs`), `ast/lexer.rs`,
the token parts of `processed_source.rs`, `comment_config.rs`,
`directive_comment.rs`, `magic_comment.rs`, `script/compare_ast` and
`script/dump_*`.

## Done

- `processed_source.tokens` works again, in safe Rust, on crates.io
  `ruby-prism 2.0.0-rc.2`. `ast/token_builder.rs` rebuilds the stream
  `Prism.lex` (prism 1.9, what RuboCop uses) produces, and feeds it to the
  existing `lexer.rs` port of `Prism::Translation::Parser::Lexer`:
  - a `Visit` walk emits a token for every location the tree records, typed
    by its node (modifier keywords, `KEYWORD_DO_LOOP`, `USTAR`,
    `UAMPERSAND`, `BRACKET_LEFT_ARRAY`, `UMINUS_NUM`, labels, symbols,
    method names, ...);
  - comments and `=begin` documents come from the parse result, `__END__`
    from `data_loc`;
  - the gaps are scanned: in code they hold whitespace, `,`, `;`, `\`
    continuations and newlines; inside literals they are `STRING_CONTENT`
    (split per line in `<<~` bodies, by whitespace with `WORDS_SEP` in
    `%w`/`%i`). The scanner tracks Prism's lexer state the way `parser_lex`
    does, which decides `NEWLINE` vs `IGNORED_NEWLINE` (including the
    leading-`.` lookahead past comment lines) and `PARENTHESIS_LEFT` vs
    `PARENTHESIS_LEFT_PARENTHESES`;
  - heredoc bodies are emitted right after their `HEREDOC_START`, then the
    rest of the line, as Prism lexes them.
- `oxcop --dump-prism-tokens FILE...` prints the rebuilt stream (and any
  gap text the scanner could not classify, as `?unknown OFFSET`);
  `script/dump_prism_tokens` prints `Prism.lex` in the same format;
  `script/compare_ast --prism-tokens DIR...` diffs them.
- `CommentConfig` (comment-only line detection for directives) and
  `FrozenStringLiteral#leading_comment_lines` read the real tokens again.

## Parity

| corpus | `--prism-tokens` (vs `Prism.lex`) | `--tokens` (vs RuboCop) |
| --- | --- | --- |
| `../code` (5676 files) | 0 differ | 0 differ |
| rubocop `lib/` (946) | 0 differ | 0 differ |
| rubocop `spec/` (827) | 0 differ | 0 differ |

Also checked with `--prism-tokens` on ~6,500 files of installed gems
(rails, brakeman, rbs, yard, sorbet-runtime, rspec, rubygems, ...): 0
differ; `--tokens` on ~1,000 of them: 0 differ.

`Token#text` of strings that aren't valid UTF-8 has U+FFFD; the raw bytes
are in `Token::raw` (what `--dump-tokens` prints). Rational and imaginary
token text is `Rational#to_s`/`Complex#to_s`, as in RuboCop.

## Known gaps

Not seen in any corpus, so not handled: a string literal that spans lines
on the same line as a heredoc start (`foo(<<A, "x\ny")`, where Prism splits
the string's content around the heredoc body), and `\` line continuations on
a heredoc start line.

## Left

- Land (blocked while main's full gate is red on the rubocop-rails config;
  `layout-b` has cherry-picked the first commit).
