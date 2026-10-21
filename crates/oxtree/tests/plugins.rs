//! Ports of syntax_tree 6.3.0's plugin tests (vendor/syntax_tree/test/plugin):
//! `single_quotes_test.rb`, `trailing_comma_test.rb` and
//! `disable_auto_ternary_test.rb`. Each plugin is a `format::Options` field,
//! as the `Formatter::Options.new(...)` calls in the tests are.

use oxtree::format::Options;

/// The tests' `assert_format`: `Formatter.new(source, [], options:)` at the
/// default print width (80), then `flush` and `output.join`.
#[track_caller]
fn assert_format(options: Options, expected: &str, source: &str) {
    let actual = oxtree::format(source, 80, options).expect("parses");
    assert_eq!(actual, expected);
}

// single_quotes_test.rb

#[track_caller]
fn assert_single_quotes(expected: &str, source: &str) {
    let options = Options {
        quote: "'",
        ..Options::default()
    };
    assert_format(options, expected, source);
}

#[test]
fn single_quotes_empty_string_literal() {
    assert_single_quotes("''\n", "\"\"");
}

#[test]
fn single_quotes_character_literal_with_double_quote() {
    assert_single_quotes("'\"'\n", "?\"");
}

#[test]
fn single_quotes_character_literal_with_singlee_quote() {
    assert_single_quotes("'\\''\n", "?'");
}

#[test]
fn single_quotes_string_literal() {
    assert_single_quotes("'string'\n", "\"string\"");
}

#[test]
fn single_quotes_string_literal_with_interpolation() {
    assert_single_quotes("\"#{foo}\"\n", "\"#{foo}\"\n");
}

#[test]
fn single_quotes_dyna_symbol() {
    assert_single_quotes(":'symbol'\n", ":\"symbol\"");
}

#[test]
fn single_quotes_single_quote_in_string() {
    assert_single_quotes("\"str'ing\"\n", "\"str'ing\"\n");
}

#[test]
fn single_quotes_label() {
    assert_single_quotes(
        "{ foo => foo, :'bar' => bar }\n",
        "{ foo => foo, \"bar\": bar }",
    );
}

// trailing_comma_test.rb

#[track_caller]
fn assert_trailing_comma(expected: &str, source: &str) {
    let options = Options {
        trailing_comma: true,
        ..Options::default()
    };
    assert_format(options, expected, source);
}

fn a80() -> String {
    "a".repeat(80)
}

#[test]
fn trailing_comma_arg_paren_flat() {
    assert_trailing_comma("foo(a)\n", "foo(a)\n");
}

#[test]
fn trailing_comma_arg_paren_break() {
    let a = a80();
    assert_trailing_comma(&format!("foo(\n  {a},\n)\n"), &format!("foo({a})\n"));
}

#[test]
fn trailing_comma_arg_paren_block() {
    let a = a80();
    assert_trailing_comma(&format!("foo(\n  &{a}\n)\n"), &format!("foo(&{a})\n"));
}

#[test]
fn trailing_comma_arg_paren_command() {
    let a = a80();
    assert_trailing_comma(&format!("foo(\n  bar {a}\n)\n"), &format!("foo(bar {a})\n"));
}

#[test]
fn trailing_comma_arg_paren_command_call() {
    let a = a80();
    assert_trailing_comma(
        &format!("foo(\n  bar.baz {a}\n)\n"),
        &format!("foo(bar.baz {a})\n"),
    );
}

#[test]
fn trailing_comma_array_literal_flat() {
    assert_trailing_comma("[a]\n", "[a]\n");
}

#[test]
fn trailing_comma_array_literal_break() {
    let a = a80();
    assert_trailing_comma(&format!("[\n  {a},\n]\n"), &format!("[{a}]\n"));
}

#[test]
fn trailing_comma_hash_literal_flat() {
    assert_trailing_comma("{ a: a }\n", "{ a: a }\n");
}

#[test]
fn trailing_comma_hash_literal_break() {
    let a = a80();
    assert_trailing_comma(
        &format!("{{\n  a:\n    {a},\n}}\n"),
        &format!("{{ a: {a} }}\n"),
    );
}

// disable_auto_ternary_test.rb

#[track_caller]
fn assert_disable_auto_ternary(expected: &str) {
    let options = Options {
        disable_auto_ternary: true,
        ..Options::default()
    };
    assert_format(options, expected, expected);
}

#[test]
fn disable_auto_ternary_short_if_else_unchanged() {
    assert_disable_auto_ternary("if true\n  1\nelse\n  2\nend\n");
}

#[test]
fn disable_auto_ternary_short_ternary_unchanged() {
    assert_disable_auto_ternary("true ? 1 : 2\n");
}

/// Not upstream: without the plugin the same `if` becomes a ternary, so the
/// test above checks the option rather than a no-op.
#[test]
fn auto_ternary_without_the_plugin() {
    assert_format(
        Options::default(),
        "true ? 1 : 2\n",
        "if true\n  1\nelse\n  2\nend\n",
    );
}
