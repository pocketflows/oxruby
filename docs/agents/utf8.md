# utf8: invalid source encodings

## What changed

A Ruby file whose bytes aren't valid in its encoding used to be decoded with
replacement characters and inspected like any other file. It now fails the
way RuboCop's does (0ec1951, "Fail on invalid source encodings as RuboCop
does"):

- `source_encoding::decode` returns the error `Parser::Source::Buffer#source=`
  raises. That covers invalid UTF-8 (no magic comment, a UTF-8 comment or a
  BOM), invalid bytes in US-ASCII and in the encodings encoding_rs converts,
  and encoding names Ruby doesn't know. Names are looked up in Ruby 3.3's
  `Encoding.name_list`, which is embedded, so aliases use Ruby's canonical
  name (`sjis` is reported as Windows-31J). `decode_lossy` keeps the old
  replacing behaviour for `--dump-ast` and the shim's `--prior` sources.
- `ProcessedSource::with_parser_error` (no AST, comments or tokens) makes
  `valid_syntax()` false. Lint/Syntax reports the error through
  `beautify_message` (`String#capitalize` plus a period) as a global offense.
- The runner investigates such a file once, skips the redundant-disable
  pass, never writes the file, and with `--stdin -a` echoes the input
  unchanged.
- Prism ignores the magic encoding comment unless the source is binary.
  `Prism::Translation::Parser` passes `encoding: false` for this, because the
  buffer is already UTF-8. Before this change, a valid Shift_JIS file
  crashed Lint/Syntax ("An error occurred while Lint/Syntax cop was
  inspecting"), because Prism re-read the transcoded text as Shift_JIS.
- `--only X` always includes Lint/Syntax (`Options#add_cop_selection_csv_option`).
  `--except Lint/Syntax` or `--except Syntax` is an options error. Neither
  applies under `--raw-config`. Before this change, `--only Style/StringLiterals`
  on a file with a syntax error printed nothing and exited 0.
- Tests: `crates/oxcop/tests/invalid_encoding.rs` runs the binary on
  fixtures, and `source_encoding.rs` has unit tests for the messages.

## Measured RuboCop 1.91.0 behaviour (Ruby 3.3.11)

These were measured on fixtures in a scratch directory, by running `bundle exec rubocop` from `../code`.

| Input | Result |
| --- | --- |
| Invalid UTF-8 anywhere (code, string, comment), no magic comment | one offense, `Lint/Syntax`, severity `fatal`, message `Invalid byte sequence in utf-8.`, `NO_LOCATION` |
| `# encoding: utf-8`, `# -*- coding: utf-8 -*-`, or a BOM, plus invalid bytes | same |
| `# encoding: ISO-8859-1` / `binary` / `Shift_JIS` (valid bytes) | inspected normally |
| `# encoding: us-ascii` + `\xC3` | `"\xc3" on us-ascii.` |
| `# encoding: Shift_JIS` + `\xFF` | `"\xff" on shift_jis.` (`sjis`: `... on windows-31j.`) |
| `# encoding: Shift_JIS` + `\x81"` | `"\x81" followed by "\"" on shift_jis.` |
| `# encoding: EUC-JP` + `\x8F"` | `"\x8f" followed by "\"" on euc-jp.` |
| `# encoding: EUC-JP`, file ending in `\xA4` | `Incomplete "\xa4" on euc-jp.` |
| `# encoding: windows-1253` + `\xAA` | `"\xaa" to utf-8 in conversion from windows-1253 to utf-8.` |
| `# encoding: bogus-enc` | `Unknown encoding name - bogus-enc.` |
| `# encoding: UTF-16` / `UTF-7` (dummy encodings) | RuboCop crashes (`incompatible character encodings`) |

The message is the Ruby error message passed through `String#capitalize`, so
everything after the first character is lowercased.

For every parser-error case:

- **Location.** The offense is global: json `location` is
  `{start_line: 1, start_column: 1, last_line: 1, last_column: 1, length: 0, line: 1, column: 1}`,
  with `"corrected": false, "correctable": false`.
- **Formatters.** emacs and clang print `path:1:1: F: Lint/Syntax: Invalid byte sequence in utf-8.`.
  clang prints no source line or caret. simple prints `F:  1:  1: Lint/Syntax: ...`.
  progress prints `F`, then the offense, then `1 file inspected, 1 offense detected`.
- **Exit code.** 1, and nothing is printed on stderr.
- **Other cops.** No other cop runs, with or without `--only`. `--only Style/StringLiterals`
  still reports Lint/Syntax. `--except Lint/Syntax` prints
  `Syntax checking cannot be turned off.` and exits 2.
- **Autocorrect.** `-a` and `-A` leave the file byte-for-byte unchanged and exit 1.
- **stdin.** `--stdin` gives the same offense under the stdin path. With `-a`,
  it prints the report, then `====================`, then the original bytes unchanged.

oxcop now matches all of the above: progress, simple, emacs, clang and json
output and exit codes are identical, apart from the dummy-encoding crash.
The full gate passed: every ported spec and `script/compare_with_rubocop`
on `../code` with 0 mismatched files.
The vendored `spec/rubocop/cop/lint/syntax_spec.rb` has a "with a parser
error" example (`# \xf9`). It runs RuboCop's own Commissioner, not the
binary ("passed without invoking oxcop"), so the Rust test above is what
pins oxcop. The vendored specs that contain `\xff` escapes (percent_string_array,
mixed_case_range, file_null, inclusive_language, interpolation_check,
nested_percent_literal, string_concatenation) and the magic-comment specs
still pass.

## Left

- Messages for invalid bytes in non-UTF-8 encodings are derived from
  encoding_rs's malformed-sequence report, so Ruby's transcoder wording
  ("followed by", "incomplete", undefined conversion) is approximated. The
  cases in the table match. Multi-byte sequences longer than two bytes, and
  WHATWG mappings that differ from Ruby's (for example, `ISO-8859-9` is read
  as windows-1254), may word or flag errors differently.
- Encodings Ruby knows but encoding_rs doesn't (macRoman, IBM437, the dummy
  UTF-16/UTF-7 family, ...) are still decoded leniently as UTF-8. For the
  dummy encodings, RuboCop itself crashes.
- `docs/agents/runtime.md` still says "No comment means UTF-8 (invalid bytes
  are replaced ...)". That is now stale, for the coordinator to fold in.
