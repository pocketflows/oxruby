//! Port of `lib/rubocop/cop/lint/misplaced_magic_comment.rb`.

use crate::cop::mixin::range_help::{
    range_by_whole_lines, range_with_surrounding_space, Side, SurroundingSpace,
};
use crate::cop::{Corrector, Cop, Investigation};
use crate::magic_comment::MagicComment;
use crate::processed_source::{Comment, ProcessedSource};

const MSG_ENCODING: &str = "The `encoding` magic comment is ignored unless placed on the first \
                            line (or below a shebang on the first line).";
const MSG_AFTER_CODE: &str = "The `%<directive>s` magic comment is ignored after any code.";
const MSG_ABOVE_SHEBANG: &str = "A magic comment above a shebang renders the shebang ineffective.";

/// `Encoding.name_list` of Ruby 3.3, downcased: `Encoding.find` is
/// case-insensitive.
const ENCODING_NAMES: &[&str] = &[
    "646", "ansi_x3.4-1968", "ascii", "ascii-8bit", "big5", "big5-hkscs",
    "big5-hkscs:2008", "big5-uao", "binary", "cesu-8", "cp1250", "cp1251",
    "cp1252", "cp1253", "cp1254", "cp1255", "cp1256", "cp1257",
    "cp1258", "cp437", "cp50220", "cp50221", "cp51932", "cp65000",
    "cp65001", "cp720", "cp737", "cp775", "cp850", "cp852",
    "cp855", "cp857", "cp860", "cp861", "cp862", "cp863",
    "cp864", "cp865", "cp866", "cp869", "cp874", "cp878",
    "cp932", "cp936", "cp949", "cp950", "cp951", "cswindows31j",
    "ebcdic-cp-us", "emacs-mule", "euc-cn", "euc-jis-2004", "euc-jisx0213", "euc-jp",
    "euc-jp-ms", "euc-kr", "euc-tw", "euccn", "eucjp", "eucjp-ms",
    "euckr", "euctw", "external", "filesystem", "gb12345", "gb18030",
    "gb1988", "gb2312", "gbk", "ibm037", "ibm437", "ibm720",
    "ibm737", "ibm775", "ibm850", "ibm852", "ibm855", "ibm857",
    "ibm860", "ibm861", "ibm862", "ibm863", "ibm864", "ibm865",
    "ibm866", "ibm869", "internal", "iso-2022-jp", "iso-2022-jp-2", "iso-2022-jp-kddi",
    "iso-8859-1", "iso-8859-10", "iso-8859-11", "iso-8859-13", "iso-8859-14", "iso-8859-15",
    "iso-8859-16", "iso-8859-2", "iso-8859-3", "iso-8859-4", "iso-8859-5", "iso-8859-6",
    "iso-8859-7", "iso-8859-8", "iso-8859-9", "iso2022-jp", "iso2022-jp2", "iso8859-1",
    "iso8859-10", "iso8859-11", "iso8859-13", "iso8859-14", "iso8859-15", "iso8859-16",
    "iso8859-2", "iso8859-3", "iso8859-4", "iso8859-5", "iso8859-6", "iso8859-7",
    "iso8859-8", "iso8859-9", "koi8-r", "koi8-u", "locale", "maccenteuro",
    "maccroatian", "maccyrillic", "macgreek", "maciceland", "macjapan", "macjapanese",
    "macroman", "macromania", "macthai", "macturkish", "macukraine", "pck",
    "shift_jis", "sjis", "sjis-docomo", "sjis-kddi", "sjis-softbank", "stateless-iso-2022-jp",
    "stateless-iso-2022-jp-kddi", "tis-620", "ucs-2be", "ucs-4be", "ucs-4le", "us-ascii",
    "utf-16", "utf-16be", "utf-16le", "utf-32", "utf-32be", "utf-32le",
    "utf-7", "utf-8", "utf-8-hfs", "utf-8-mac", "utf8-docomo", "utf8-kddi",
    "utf8-mac", "utf8-softbank", "windows-1250", "windows-1251", "windows-1252", "windows-1253",
    "windows-1254", "windows-1255", "windows-1256", "windows-1257", "windows-1258", "windows-31j",
    "windows-874",
];

#[derive(Default)]
pub struct MisplacedMagicComment;

impl Cop for MisplacedMagicComment {
    fn name(&self) -> &'static str {
        "Lint/MisplacedMagicComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.buffer_source().is_empty() {
            return;
        }
        check_magic_comment_above_shebang(cx);
        for comment in ps.comments() {
            check_comment(cx, comment);
        }
    }
}

fn check_comment(cx: &mut Investigation<'_, '_>, comment: &Comment<'_>) {
    if !magic_comment_shaped(comment.text) {
        return;
    }
    let magic_comment = MagicComment::parse(comment.text);
    if !magic_comment.valid() {
        return;
    }
    if let Some(encoding) = magic_comment.encoding() {
        if !known_encoding(&encoding) {
            return;
        }
        check_encoding_comment(cx, comment);
    } else if magic_comment.frozen_string_literal_specified() {
        check_top_block_comment(cx, comment, "frozen_string_literal");
    }
}

/// The Emacs and Vim regexps in `MagicComment` match anywhere inside a
/// comment, which would flag documentation that merely quotes a magic
/// comment (e.g. `#   # -*- coding: UTF-8 -*-`). Require the magic
/// comment to start right after the `#`.
fn magic_comment_shaped(text: &str) -> bool {
    // /\A#(?![^#]*#)/
    text.strip_prefix('#').is_some_and(|rest| !rest.contains('#'))
}

/// Prose that happens to open with `Encoding:` parses as an encoding
/// comment (`# Encoding: force given encoding` yields `force`). Ruby
/// ignores a misplaced encoding comment entirely, so a name no encoding
/// answers to means the line is a comment, not a directive.
fn known_encoding(name: &str) -> bool {
    ENCODING_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

fn is_shebang(ps: &ProcessedSource<'_>) -> bool {
    ps.lines().first().is_some_and(|l| l.starts_with("#!"))
}

/// First line that may carry an effective encoding comment.
fn effective_encoding_line(ps: &ProcessedSource<'_>) -> usize {
    if is_shebang(ps) { 2 } else { 1 }
}

/// `first_code_token`'s `begin_pos`.
fn first_code_token(ps: &ProcessedSource<'_>) -> Option<usize> {
    ps.sorted_tokens()
        .iter()
        .find(|token| !token.is_comment())
        .map(|token| token.begin_pos())
}

fn check_encoding_comment(cx: &mut Investigation<'_, '_>, comment: &Comment<'_>) {
    let ps = cx.processed_source;
    let line = ps.line_of(comment.range.begin_pos);
    if line == effective_encoding_line(ps) && comment_starts_line(ps, comment) {
        return;
    }
    // Leave `# frozen_string_literal: true` + `# encoding: x` runs at the
    // very top to Lint/OrderedMagicComments, which already reorders them.
    if preceded_only_by_magic_comments(ps, comment) {
        return;
    }
    cx.add_offense_with_correction(comment.range, MSG_ENCODING, |corrector| {
        move_comment(corrector, ps, comment, effective_encoding_line(ps));
    });
}

fn check_top_block_comment(cx: &mut Investigation<'_, '_>, comment: &Comment<'_>, directive: &str) {
    let ps = cx.processed_source;
    let Some(first_code_token) = first_code_token(ps) else {
        return;
    };
    if comment.range.begin_pos <= first_code_token {
        return;
    }
    let message = MSG_AFTER_CODE.replace("%<directive>s", directive);
    cx.add_offense_with_correction(comment.range, &message, |corrector| {
        move_comment(corrector, ps, comment, effective_encoding_line(ps));
    });
}

fn check_magic_comment_above_shebang(cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    let shebang_comment = ps.comments().iter().find(|comment| {
        ps.line_of(comment.range.begin_pos) > 1
            && comment.text.starts_with("#!")
            && ps.column_of(comment.range.begin_pos) == 0
    });
    let Some(shebang_comment) = shebang_comment else {
        return;
    };
    // Only flag when what precedes the shebang is magic comments -
    // a `#!` comment deep inside a file is not an intended shebang.
    if !preceded_only_by_magic_comments(ps, shebang_comment) {
        return;
    }
    cx.add_offense(shebang_comment.range, MSG_ABOVE_SHEBANG);
}

fn preceded_only_by_magic_comments(ps: &ProcessedSource<'_>, comment: &Comment<'_>) -> bool {
    (1..ps.line_of(comment.range.begin_pos)).all(|line_number| {
        let text = ps.lines()[line_number - 1];
        (line_number == 1 && text.starts_with("#!")) || MagicComment::parse(text).valid()
    })
}

fn comment_starts_line(ps: &ProcessedSource<'_>, comment: &Comment<'_>) -> bool {
    ps.lines()[ps.line_of(comment.range.begin_pos) - 1]
        .trim_start()
        .starts_with('#')
}

fn move_comment(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    comment: &Comment<'_>,
    target_line: usize,
) {
    let removal_range = if comment_starts_line(ps, comment) {
        range_by_whole_lines(ps, comment.range, true)
    } else {
        range_with_surrounding_space(
            ps.buffer_source(),
            comment.range,
            SurroundingSpace::side(Side::Left),
        )
    };
    corrector.remove(removal_range);
    let target_range = ps.line_range(target_line);
    if ps.line_of(comment.range.begin_pos) < target_line {
        corrector.insert_after(target_range, format!("\n{}", comment.text));
    } else {
        corrector.insert_before(target_range, format!("{}\n", comment.text));
    }
}
