//! Ports of w_syntax_tree-erb 0.12.0's test suite (vendor/syntax_tree-erb/test):
//! `formatting_test.rb` (the `fixture/*` pairs), `erb_test.rb` and
//! `html_test.rb`.
//!
//! Differences from upstream:
//! - `assert_formatting` also checks `pretty_inspect` of the parsed tree.
//!   oxtree has no `pretty_print` for ERB nodes, so that part is skipped.
//! - Upstream's parse-error tests only assert inside `rescue`, so they pass
//!   when nothing is raised. Here the error is required.
//! - Assertions on Ruby node classes (`assert_instance_of(ERB::Doctype, ..)`)
//!   become `matches!` on the equivalent `Elem` variant. `Document#location`
//!   doesn't exist in oxtree, so `test_empty_file` checks only the elements.

use std::path::PathBuf;

use oxtree::erb::parser::{Attr, BlockKind, Elem, StringPart};

fn format(source: &str) -> String {
    oxtree::erb::format(source, oxtree::erb::MAX_WIDTH)
        .unwrap_or_else(|e| panic!("parse error {e} ({}:{})\n{source}", e.lineno, e.column))
}

/// `TestCase#assert_formatting` from test_helper.rb.
#[track_caller]
fn assert_formatting(source: &str, expected: &str) {
    let formatted = format(source);
    assert_eq!(formatted, expected, "Failed first");
    let formatted_twice = format(&formatted);
    assert_eq!(formatted_twice, expected, "Failed second");
    // Check that the expected output parses (upstream: pretty_inspect).
    oxtree::erb::parse(expected).expect("expected output parses");
}

/// `ERB.parse(example)` followed by upstream's `rescue ParseError` block.
#[track_caller]
fn assert_parse_error(source: &str, lineno: usize, column: usize, message: &str) {
    let error = match oxtree::erb::parse(source) {
        Ok(_) => panic!("expected a parse error for:\n{source}"),
        Err(error) => error,
    };
    assert!(
        error.message.contains(message),
        "message {:?} doesn't match /{message}/",
        error.message
    );
    assert_eq!(
        (error.lineno, error.column),
        (lineno, column),
        "line and column of {:?}",
        error.message
    );
}

fn elements(source: &str) -> Vec<Elem> {
    oxtree::erb::parse(source).expect("parses").elements
}

// formatting_test.rb

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/syntax_tree-erb/test/fixture")
}

#[track_caller]
fn assert_fixture(name: &str) {
    let read = |suffix: &str| {
        let path = fixture_dir().join(format!("{name}_{suffix}.html.erb"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let (source, expected) = (read("unformatted"), read("formatted"));
    let formatted = format(&source);
    assert_eq!(formatted, expected, "Failed to format {name}");
    let formatted_twice = format(&formatted);
    assert_eq!(formatted_twice, expected, "{name} is not idempotent");
    oxtree::erb::parse(&expected).expect("expected output parses");
}

#[test]
fn every_fixture_is_tested() {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixture directory")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter_map(|file| file.strip_suffix("_unformatted.html.erb").map(String::from))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "block",
            "case",
            "erb_syntax",
            "if_statements",
            "javascript_frameworks",
            "layout",
            "nested_html"
        ],
        "a fixture was added upstream: add a test for it"
    );
}

#[test]
fn fixture_block() {
    assert_fixture("block");
}

#[test]
fn fixture_erb_syntax() {
    assert_fixture("erb_syntax");
}

#[test]
fn fixture_nested_html() {
    assert_fixture("nested_html");
}

#[test]
fn fixture_if_statements() {
    assert_fixture("if_statements");
}

#[test]
fn fixture_javascript_frameworks() {
    assert_fixture("javascript_frameworks");
}

#[test]
fn fixture_case_statements() {
    assert_fixture("case");
}

#[test]
fn fixture_layout() {
    assert_fixture("layout");
}

// erb_test.rb

#[test]
fn erb_empty_file() {
    assert!(elements("").is_empty());
}

#[test]
fn erb_missing_erb_end_tag() {
    let example = r#"<ul>
  <% if condition %>
    <li>A</li>
    <li>B</li>
    <li><%= "C" %></li>
</ul>
"#;
    assert_parse_error(example, 2, 2, "No matching ERB-tag for the <% if %>");
}

#[test]
fn erb_missing_erb_block_end_tag() {
    let example = "<% no_end_tag do %>\n  <h1>What</h1>\n";
    assert_parse_error(example, 1, 0, "No matching <% end %> for the <% do %>");
}

#[test]
fn erb_missing_erb_case_end_tag() {
    let example =
        "<% case variabel %>\n<% when 1 %>\n  Hello\n<% when 2 %>\n  World\n<h1>What</h1>\n";
    assert_parse_error(example, 4, 0, "No matching ERB-tag for the <% when %>");
}

#[test]
fn erb_code_with_non_ascii() {
    let elements = elements("<% \"Påäööööö\" %>");
    assert_eq!(elements.len(), 1);
    assert!(matches!(elements[0], Elem::Erb(_)));
}

#[test]
fn erb_syntax_error() {
    let example = r#"<ul>
  <% if @items.each do |i| %>
    <li><%= i %></li>
  <% end.blank? %>
    <li>No items</li>
  <% end %>
</ul>
"#;
    assert_parse_error(example, 4, 7, "Could not parse ERB-tag");
}

#[test]
fn erb_syntax_error2() {
    let example = r#"<%= content_tag :header do %>
  <div class="flex-1 min-w-0">
    <h2>
      <%= yield :page_header %>
    </h2>
    <%= content_tag :div do %>
      <%= yield :page_subheader %>
    <% end if content_for?(:page_subheader) %>
  </div>
  <%= content_tag :div do %>
    <%= yield :page_actions %>
  <% end if content_for?(:page_actions) %>
<% end if content_for?(:page_header) %>
"#;
    assert_parse_error(example, 8, 9, "Could not parse ERB-tag");
}

#[test]
fn erb_if_and_end_in_same_output_tag_short() {
    assert_formatting("<%= if true\n  what\nend %>", "<%= what if true %>\n");
}

#[test]
fn erb_if_and_end_in_same_tag() {
    assert_formatting(
        "Hello\n<% if true then this elsif false then that else maybe end %>\n<h1>Hey</h1>",
        "Hello\n<% if true\n  this\nelsif false\n  that\nelse\n  maybe\nend %>\n<h1>Hey</h1>\n",
    );
}

#[test]
fn erb_inside_html_tag() {
    assert_formatting(
        "<div <% if eeee then \"b\" else c end %>></div>",
        "<div <% eeee ? \"b\" : c %>></div>\n",
    );
}

#[test]
fn erb_long_if_statement() {
    assert_formatting(
        "<%=number_to_percentage(@reports&.first&.stability*100,precision: 1) if @reports&.first&.other&.stronger&.longer %>",
        "<%= if @reports&.first&.other&.stronger&.longer\n  number_to_percentage(@reports&.first&.stability * 100, precision: 1)\nend %>\n",
    );
}

#[test]
fn erb_else_if_statement() {
    assert_formatting(
        "<%if this%>\n  <h1>A</h1>\n<%elsif that%>\n  <h1>B</h1>\n<%else%>\n  <h1>C</h1>\n<%end%>",
        "<% if this %>\n  <h1>A</h1>\n<% elsif that %>\n  <h1>B</h1>\n<% else %>\n  <h1>C</h1>\n<% end %>\n",
    );
}

#[test]
fn erb_long_ternary() {
    assert_formatting(
        "<%= number_to_percentage(@reports&.first&.stability * 100, precision: @reports&.first&.stability ? 'Stable' : 'Unstable') %>",
        "<%= number_to_percentage(\n  @reports&.first&.stability * 100,\n  precision: @reports&.first&.stability ? \"Stable\" : \"Unstable\"\n) %>\n",
    );
}

#[test]
fn erb_text_erb_text() {
    assert_formatting(
        "<div>This is some text <%= variable %> and the special value after</div>",
        "<div>This is some text <%= variable %> and the special value after</div>\n",
    );
}

#[test]
fn erb_with_comment() {
    let source = "<%= what # This is a comment %>\n";
    assert_formatting(source, source);
}

#[test]
fn erb_only_ruby_comment() {
    let source = "<% # This should be written on one line %>\n";
    assert_formatting(source, source);
}

#[test]
fn erb_comment() {
    let source = "<%# This should be written on one line %>\n";
    assert_formatting(source, source);
}

#[test]
fn erb_multiline_comment() {
    assert_formatting(
        "<%#\n  This is the first\n     This is the second\n    This is the third %>",
        "<%#\nThis is the first\nThis is the second\nThis is the third %>\n",
    );
}

#[test]
fn erb_ternary_as_argument_without_parentheses() {
    assert_formatting(
        "<%=     f.submit( f.object.id.present?     ? t('buttons.titles.save'):t('buttons.titles.create'))   %>",
        "<%= f.submit(\n  f.object.id.present? ? t(\"buttons.titles.save\") : t(\"buttons.titles.create\")\n) %>\n",
    );
}

#[test]
fn erb_whitespace() {
    assert_formatting(
        "<%= 1 %>,<%= 2 %>What\n<%= link_to(url) do %><strong>Very long link Very long link Very long link Very long link</strong><% end %>",
        "<%= 1 %>,<%= 2 %>What\n<%= link_to(url) do %>\n  <strong>Very long link Very long link Very long link Very long link</strong>\n<% end %>\n",
    );
}

#[test]
fn erb_block_do_arguments() {
    assert_formatting(
        "<%= link_to(url) do |link, other_arg|%>Whaaaaaaat<% end %>",
        "<%= link_to(url) do |link, other_arg| %>\n  Whaaaaaaat\n<% end %>\n",
    );
}

#[test]
fn erb_newline() {
    assert_formatting(
        "<%= what if this %>\n<h1>hej</h1>",
        "<%= what if this %>\n<h1>hej</h1>\n",
    );
}

#[test]
fn erb_group_blank_line() {
    let source = "<%= hello %>\n<%= heya %>\n\n<%# breaks the group %>\n";
    assert_formatting(source, source);
}

#[test]
fn erb_empty_first_line() {
    assert_formatting("\n\n<%= what %>\n", "<%= what %>\n");
}

#[test]
fn erb_parsing_column_position() {
    let example = "<ul>
  <% if condition %>
    <li>A</li>
  <% end %>
  <!-- Comment
  about something and other
  --><%= yes %>
</ul>
";
    let elements = elements(example);
    assert_eq!(elements.len(), 1);

    let Elem::Html {
        elements: ul_elements,
        location: ul,
        ..
    } = &elements[0]
    else {
        panic!("expected an HtmlNode, got {:?}", elements[0]);
    };
    assert_eq!(ul.start_line, 1);
    assert_eq!(ul.end_line, 8);
    assert_eq!(ul.start_char, 0);
    assert_eq!(ul.start_column, 0);
    assert_eq!(ul.end_column, 5);
    assert_eq!(ul_elements.len(), 3);

    let if_node = &ul_elements[0];
    assert!(matches!(
        if_node,
        Elem::Block {
            kind: BlockKind::ErbIf,
            ..
        }
    ));
    let if_loc = if_node.location();
    assert_eq!(if_loc.start_line, 2);
    assert_eq!(if_loc.end_line, 4);
    assert_eq!(if_loc.start_column, 2);

    let comment_node = &ul_elements[1];
    assert!(matches!(comment_node, Elem::HtmlComment { .. }));
    let comment_loc = comment_node.location();
    assert_eq!(comment_loc.start_line, 5);
    assert_eq!(comment_loc.end_line, 7);
    assert_eq!(comment_loc.start_column, 2);
    assert_eq!(comment_loc.end_column, 6);
}

// html_test.rb

#[test]
fn html_wrong_end_tag() {
    let example =
        "<div>\n  <ul>\n    <li>A</li>\n    <li>B</li>\n    <li>C</li>\n    <li>D</li>\n</div>\n";
    assert_parse_error(example, 7, 0, "Expected closing tag for <ul> but got <div>");
}

#[test]
fn html_no_end_tag() {
    assert_parse_error("<h1>Hello World\n", 1, 0, "Missing closing tag for <h1>");
}

#[test]
fn html_incorrect_end_tag() {
    assert_parse_error(
        "<div>\n<h1>Hello World</h2>\n</div>\n",
        2,
        15,
        "Expected closing tag for <h1> but got <h2>",
    );
}

#[test]
fn html_unmatched_double_quote() {
    assert_parse_error(
        "<div class=\"card-\"\">Hello World</div>\n",
        1,
        31,
        "Unexpected character, <, when looking for closing double quote",
    );
}

#[test]
fn html_unmatched_single_quote() {
    assert_parse_error(
        "<div class='card-''>Hello World</div>\n",
        1,
        31,
        "Unexpected character, <, when looking for closing single quote",
    );
}

#[test]
fn html_empty_file() {
    assert_formatting("", "\n");
}

#[test]
fn html_doctype() {
    assert!(matches!(
        elements("<!DOCTYPE html>")[0],
        Elem::Doctype { .. }
    ));
    assert!(matches!(
        elements("<!doctype html>")[0],
        Elem::Doctype { .. }
    ));

    // Allow doctype to not be the first element
    let elements = elements("<% theme = \"general\" %> <!DOCTYPE html>");
    assert_eq!(elements.len(), 2);
    assert!(matches!(elements[0], Elem::Erb(_)));
    assert!(matches!(elements[1], Elem::Doctype { .. }));
}

#[test]
fn html_doctype_duplicate() {
    assert_parse_error(
        "<!DOCTYPE html>\n<h1>Hello World</h1>\n<!DOCTYPE html>\n",
        3,
        0,
        "Duplicate doctype declaration",
    );
}

#[test]
fn html_comment() {
    let source = "<!-- This is a HTML-comment -->\n";
    let elements = elements(source);
    assert_eq!(elements.len(), 1);
    assert!(matches!(elements[0], Elem::HtmlComment { .. }));

    assert_formatting(source, source);
}

#[test]
fn html_within_quotes() {
    let elements = elements("<p>This is our text \"<strong><%= @object.quote %></strong>\"</p>");
    assert_eq!(elements.len(), 1);
    let Elem::Html { elements, .. } = &elements[0] else {
        panic!("expected an HtmlNode, got {:?}", elements[0]);
    };
    let char_data = |elem: &Elem| match elem {
        Elem::CharData { value, .. } => value.value.clone(),
        other => panic!("expected CharData, got {other:?}"),
    };
    assert_eq!(char_data(elements.first().unwrap()), "This is our text \"");
    assert_eq!(char_data(elements.last().unwrap()), "\"");
}

#[test]
fn html_tag_name_at() {
    assert_parse_error("<@br />", 1, 1, "Invalid HTML-tag name @br");
}

#[test]
fn html_tag_name_colon() {
    assert_parse_error("<:br />", 1, 1, "Invalid HTML-tag name :br");
}

#[test]
fn html_tag_name_hash() {
    assert_parse_error("<#br />", 1, 1, "Invalid HTML-tag name #br");
}

#[test]
fn html_attribute_without_quotes() {
    let source = "<div class=card>Hello World</div>";
    let elements = elements(source);
    assert_eq!(elements.len(), 1);
    let Elem::Html { opening, .. } = &elements[0] else {
        panic!("expected an HtmlNode, got {:?}", elements[0]);
    };
    assert_eq!(opening.attributes.len(), 1);
    let Attr::Html(attribute) = &opening.attributes[0] else {
        panic!("expected an HtmlAttribute, got {:?}", opening.attributes[0]);
    };
    assert_eq!(attribute.key.value, "class");
    let value = attribute.value.as_ref().expect("attribute value");
    let Some(StringPart::Token(first)) = value.contents.first() else {
        panic!("expected a token, got {:?}", value.contents);
    };
    assert_eq!(first.value, "card");

    assert_formatting(source, "<div class=\"card\">Hello World</div>\n");
}

#[test]
fn html_empty_component_without_attributes() {
    assert_formatting(
        "<component-without-content>\n</component-without-content>\n",
        "<component-without-content></component-without-content>\n",
    );
}

#[test]
fn html_empty_component_with_attributes() {
    assert_formatting(
        "<three-word-component :allowed-words=\"['first', 'second', 'third', 'fourth']\" :disallowed-words=\"['fifth', 'sixth']\" >\n</three-word-component>",
        "<three-word-component\n  :allowed-words=\"['first', 'second', 'third', 'fourth']\"\n  :disallowed-words=\"['fifth', 'sixth']\"\n></three-word-component>\n",
    );
}

#[test]
fn html_keep_lines_with_text_in_block() {
    assert_formatting(
        "<h2>Hello <%= @football_team_membership.user %>,</h2>",
        "<h2>Hello <%= @football_team_membership.user %>,</h2>\n",
    );
}

#[test]
fn html_keep_lines_with_text_in_block_in_document() {
    assert_formatting("Hello <span>Name</span>!", "Hello <span>Name</span>!\n");
}

#[test]
fn html_keep_lines_with_nested_html() {
    assert_formatting(
        "<div>Hello <span>Name</span>!</div>",
        "<div>Hello <span>Name</span>!</div>\n",
    );
}

#[test]
fn html_newlines() {
    assert_formatting("Hello\n\n\n\nGoodbye!\n", "Hello\n\nGoodbye!\n");
}

#[test]
fn html_indentation() {
    assert_formatting(
        "<div>\n    <div>\n     <div>\nWhat\n</div>\n     </div>\n  </div>\n",
        "<div>\n  <div>\n    <div>What</div>\n  </div>\n</div>\n",
    );
}

#[test]
fn html_append_newlines() {
    let source = "<div>\nWhat\n</div>";
    let elements = elements(source);
    assert_eq!(elements.len(), 1);
    let Elem::Html {
        opening,
        elements: children,
        closing,
        ..
    } = &elements[0]
    else {
        panic!("expected an HtmlNode, got {:?}", elements[0]);
    };
    assert!(opening.new_line.is_some());
    assert!(children[0].new_line().is_some());
    assert!(closing.as_ref().expect("closing tag").new_line.is_none());

    assert_formatting(source, "<div>What</div>\n");
    assert_formatting("<div>What</div>", "<div>What</div>\n");
}

#[test]
fn html_self_closing_with_blank_line() {
    let source = "<meta http-equiv=\"Content-Type\" content=\"text/html; charset=UTF-8\" />\n\n<title>Test</title>\n";
    assert_formatting(source, source);
}

#[test]
fn html_tag_with_leading_and_trailing_spaces() {
    assert_formatting("<div>   What   </div>", "<div>What</div>\n");
}

#[test]
fn html_tag_with_leading_and_trailing_spaces_erb() {
    assert_formatting(
        "<div>   <%=user.name%>   </div>",
        "<div><%= user.name %></div>\n",
    );
}

#[test]
fn html_breakable_on_char_data_white_space() {
    assert_formatting(
        "You have been removed as a user from <strong><%= @company.title %></strong> by <%= @administrator.name %>.",
        "You have been removed as a user from <strong>\n  <%= @company.title %>\n</strong> by <%= @administrator.name %>.\n",
    );
}

#[test]
fn html_self_closing_group() {
    assert_formatting(
        "<link />\n<link />\n<meta />",
        "<link />\n<link />\n<meta />\n",
    );
}

#[test]
fn html_self_closing_for_void_elements() {
    assert_formatting(
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\" >",
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\" />\n",
    );
}
