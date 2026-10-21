//! How source bytes become the UTF-8 text cops inspect, and back: a port of
//! `Parser::Source::Buffer#source=` (`recognize_encoding`,
//! `reencode_string` and the `valid_encoding?` check), with rubocop-ast's
//! default of UTF-8 (`ProcessedSource#initialize`).
//!
//! - No magic encoding comment (or a UTF-8 one): the bytes must be valid
//!   UTF-8. If they aren't, `Buffer#source=` raises `EncodingError`
//!   ("invalid byte sequence in UTF-8"), which `ProcessedSource#parse`
//!   keeps as its `parser_error`: no AST, no comments, no tokens, and
//!   Lint/Syntax reports the message as a global fatal offense.
//! - `# encoding: BINARY` (`ASCII-8BIT`): Ruby keeps a binary string, where
//!   every byte is one character. Each byte becomes the character U+00XX, so
//!   reported columns and lengths count bytes as RuboCop's do, and
//!   [`encode`] turns the characters back into those bytes.
//! - Any other encoding: transcoded to UTF-8 (`encode(Encoding::UTF_8)`), as
//!   the buffer is; corrections are written as UTF-8, as RuboCop writes them.
//!   Bytes the encoding can't convert are an error, as `String#encode`'s
//!   `InvalidByteSequenceError` / `UndefinedConversionError` are.
//! - An encoding name Ruby doesn't know is an error too
//!   (`Parser::UnknownEncodingInMagicComment`).

use std::sync::LazyLock;

use regex::bytes::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceEncoding {
    #[default]
    Utf8,
    /// `ASCII-8BIT`.
    Binary,
    /// `ISO-8859-1`, which encoding_rs's labels map to windows-1252.
    Latin1,
    /// `US-ASCII`, which encoding_rs's labels map to windows-1252 too.
    UsAscii,
    /// An encoding encoding_rs converts; the second field is Ruby's name for
    /// it (`Encoding#name`), used in error messages.
    Other(&'static encoding_rs::Encoding, &'static str),
    /// An encoding Ruby knows but encoding_rs doesn't (macRoman, the dummy
    /// encodings, ...): decoded as UTF-8, replacing invalid bytes.
    Unsupported,
}

/// `Parser::Source::Buffer::ENCODING_RE`.
static ENCODING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[\s#](?:en)?coding\s*[:=]\s*(?:(utf8-mac)|([A-Za-z0-9_-]+?)(?:-unix|-dos|-mac)|([A-Za-z0-9_-]+))",
    )
    .expect("valid regex")
});

/// `Buffer.recognize_encoding`: the encoding a magic comment (or a BOM)
/// names on the first line, or the second after a shebang. `Err` is the
/// message of `Parser::UnknownEncodingInMagicComment`.
pub fn recognize_encoding(source: &[u8]) -> Result<Option<SourceEncoding>, String> {
    if source.is_empty() {
        return Ok(None);
    }
    let mut lines = source.splitn(3, |&b| b == b'\n');
    let first_line = lines.next().unwrap_or_default();
    let second_line = lines.next();
    if first_line.starts_with(b"\xEF\xBB\xBF") {
        return Ok(Some(SourceEncoding::Utf8));
    }
    let encoding_line = if first_line.starts_with(b"#!") {
        match second_line {
            Some(line) => line,
            None => return Ok(None),
        }
    } else {
        first_line
    };
    if encoding_line.first() != Some(&b'#') {
        return Ok(None);
    }
    let Some(captures) = ENCODING_RE.captures(encoding_line) else {
        return Ok(None);
    };
    let Some(name) = captures
        .get(1)
        .or_else(|| captures.get(2))
        .or_else(|| captures.get(3))
    else {
        return Ok(None);
    };
    find_encoding(&String::from_utf8_lossy(name.as_bytes()))
}

/// `Encoding.find`: `Err` ("unknown encoding name - ...") for a name Ruby
/// doesn't know.
fn find_encoding(name: &str) -> Result<Option<SourceEncoding>, String> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        // `Encoding.find("internal")` is `Encoding.default_internal`: nil.
        "internal" => return Ok(None),
        // The default external, locale and filesystem encodings: UTF-8 is
        // what RuboCop runs with.
        "external" | "locale" | "filesystem" => return Ok(Some(SourceEncoding::Utf8)),
        _ => {}
    }
    let Ok(index) = RUBY_ENCODINGS.binary_search_by(|(n, _, _)| (*n).cmp(lower.as_str())) else {
        return Err(format!("unknown encoding name - {name}"));
    };
    let (_, ruby_name, dummy) = RUBY_ENCODINGS[index];
    Ok(Some(match ruby_name {
        "ASCII-8BIT" => SourceEncoding::Binary,
        "UTF-8" | "UTF8-MAC" => SourceEncoding::Utf8,
        "US-ASCII" => SourceEncoding::UsAscii,
        "ISO-8859-1" => SourceEncoding::Latin1,
        _ if dummy => SourceEncoding::Unsupported,
        _ => match encoding_rs::Encoding::for_label(lower.as_bytes())
            .or_else(|| encoding_rs::Encoding::for_label(ruby_name.as_bytes()))
        {
            Some(encoding) if encoding == encoding_rs::UTF_8 => SourceEncoding::Utf8,
            Some(encoding) => SourceEncoding::Other(encoding, ruby_name),
            None => SourceEncoding::Unsupported,
        },
    }))
}

/// The text cops see for `bytes`, and its encoding (for [`encode`]). `Err`
/// is the message of the error `Buffer#source=` raises (which
/// `ProcessedSource` keeps as `parser_error`), before Lint/Syntax beautifies
/// it.
pub fn decode(bytes: &[u8]) -> Result<(String, SourceEncoding), String> {
    let encoding = recognize_encoding(bytes)?.unwrap_or_default();
    let text = match encoding {
        SourceEncoding::Utf8 => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => return Err("invalid byte sequence in UTF-8".to_owned()),
        },
        SourceEncoding::Binary | SourceEncoding::Latin1 => {
            bytes.iter().map(|&b| char::from(b)).collect()
        }
        SourceEncoding::UsAscii => match bytes.iter().find(|b| !b.is_ascii()) {
            Some(&byte) => return Err(format!("{} on US-ASCII", dump(&[byte]))),
            None => bytes.iter().map(|&b| char::from(b)).collect(),
        },
        SourceEncoding::Other(encoding, ruby_name) => transcode(bytes, encoding, ruby_name)?,
        SourceEncoding::Unsupported => String::from_utf8_lossy(bytes).into_owned(),
    };
    Ok((text, encoding))
}

/// [`decode`], replacing what can't be decoded instead of failing: for
/// tools (`--dump-ast`) and the spec shim's prior sources.
pub fn decode_lossy(bytes: &[u8]) -> (String, SourceEncoding) {
    if let Ok(decoded) = decode(bytes) {
        return decoded;
    }
    match recognize_encoding(bytes).ok().flatten() {
        Some(SourceEncoding::Other(encoding, _)) => (
            encoding.decode_without_bom_handling(bytes).0.into_owned(),
            SourceEncoding::Other(encoding, ""),
        ),
        _ => (
            String::from_utf8_lossy(bytes).into_owned(),
            SourceEncoding::Utf8,
        ),
    }
}

/// `input.force_encoding(encoding).encode(Encoding::UTF_8)`, failing with
/// the message of the first `Encoding::InvalidByteSequenceError` (or, in a
/// single-byte encoding, `UndefinedConversionError`) Ruby would raise.
fn transcode(
    bytes: &[u8],
    encoding: &'static encoding_rs::Encoding,
    ruby_name: &str,
) -> Result<String, String> {
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let capacity = decoder
        .max_utf8_buffer_length_without_replacement(bytes.len())
        .unwrap_or(bytes.len() * 3);
    let mut text = String::with_capacity(capacity);
    let (result, read) = decoder.decode_to_string_without_replacement(bytes, &mut text, true);
    match result {
        encoding_rs::DecoderResult::InputEmpty => Ok(text),
        encoding_rs::DecoderResult::OutputFull => {
            // Not reached: the buffer is the decoder's maximum.
            Ok(encoding.decode_without_bom_handling(bytes).0.into_owned())
        }
        encoding_rs::DecoderResult::Malformed(length, after) => {
            let end = read - usize::from(after);
            let start = end - usize::from(length);
            let error_bytes = &bytes[start..end];
            if encoding.is_single_byte() {
                // Every byte is a character of a single-byte encoding; one
                // without a Unicode mapping is an undefined conversion.
                return Err(format!(
                    "{} to UTF-8 in conversion from {ruby_name} to UTF-8",
                    dump(error_bytes)
                ));
            }
            // A valid start of a sequence that the next byte (or the end of
            // the file) cuts short: Ruby's transcoder reports that byte as
            // read again ("followed by"), or the input as incomplete.
            let mut probe = encoding.new_decoder_without_bom_handling();
            let mut sink = String::with_capacity(capacity);
            let prefix = matches!(
                probe.decode_to_string_without_replacement(error_bytes, &mut sink, false),
                (encoding_rs::DecoderResult::InputEmpty, _)
            );
            let followed_by = if after > 0 {
                Some(&bytes[end..read])
            } else if prefix {
                bytes.get(end..=end)
            } else {
                None
            };
            Err(match followed_by {
                Some(following) => format!(
                    "{} followed by {} on {ruby_name}",
                    dump(error_bytes),
                    dump(following)
                ),
                None if prefix => format!("incomplete {} on {ruby_name}", dump(error_bytes)),
                None => format!("{} on {ruby_name}", dump(error_bytes)),
            })
        }
    }
}

/// `String#dump` of a binary string, as the transcoder's error messages
/// show bytes.
fn dump(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &byte in bytes {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x0c => out.push_str("\\f"),
            0x0b => out.push_str("\\v"),
            0x08 => out.push_str("\\b"),
            0x07 => out.push_str("\\a"),
            0x1b => out.push_str("\\e"),
            0x20..=0x7e => out.push(char::from(byte)),
            _ => out.push_str(&format!("\\x{byte:02X}")),
        }
    }
    out.push('"');
    out
}

/// The bytes to write for (corrected) `text` of a source decoded as
/// `encoding`.
pub fn encode(text: &str, encoding: SourceEncoding) -> Vec<u8> {
    match encoding {
        SourceEncoding::Binary => {
            let mut bytes = Vec::with_capacity(text.len());
            for c in text.chars() {
                match u8::try_from(u32::from(c)) {
                    Ok(byte) => bytes.push(byte),
                    // A correction inserted a character beyond one byte.
                    Err(_) => bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
                }
            }
            bytes
        }
        _ => text.as_bytes().to_vec(),
    }
}

/// `Encoding.name_list` of Ruby 3.3 (without `external`, `locale`,
/// `filesystem` and `internal`), downcased and sorted, with each name's
/// `Encoding#name` and `dummy?`.
const RUBY_ENCODINGS: &[(&str, &str, bool)] = &[
    ("646", "US-ASCII", false),
    ("ansi_x3.4-1968", "US-ASCII", false),
    ("ascii", "US-ASCII", false),
    ("ascii-8bit", "ASCII-8BIT", false),
    ("big5", "Big5", false),
    ("big5-hkscs", "Big5-HKSCS", false),
    ("big5-hkscs:2008", "Big5-HKSCS", false),
    ("big5-uao", "Big5-UAO", false),
    ("binary", "ASCII-8BIT", false),
    ("cesu-8", "CESU-8", false),
    ("cp1250", "Windows-1250", false),
    ("cp1251", "Windows-1251", false),
    ("cp1252", "Windows-1252", false),
    ("cp1253", "Windows-1253", false),
    ("cp1254", "Windows-1254", false),
    ("cp1255", "Windows-1255", false),
    ("cp1256", "Windows-1256", false),
    ("cp1257", "Windows-1257", false),
    ("cp1258", "Windows-1258", false),
    ("cp437", "IBM437", false),
    ("cp50220", "CP50220", true),
    ("cp50221", "CP50221", true),
    ("cp51932", "CP51932", false),
    ("cp65000", "UTF-7", true),
    ("cp65001", "UTF-8", false),
    ("cp720", "IBM720", false),
    ("cp737", "IBM737", false),
    ("cp775", "IBM775", false),
    ("cp850", "CP850", false),
    ("cp852", "CP852", false),
    ("cp855", "CP855", false),
    ("cp857", "IBM857", false),
    ("cp860", "IBM860", false),
    ("cp861", "IBM861", false),
    ("cp862", "IBM862", false),
    ("cp863", "IBM863", false),
    ("cp864", "IBM864", false),
    ("cp865", "IBM865", false),
    ("cp866", "IBM866", false),
    ("cp869", "IBM869", false),
    ("cp874", "Windows-874", false),
    ("cp878", "KOI8-R", false),
    ("cp932", "Windows-31J", false),
    ("cp936", "GBK", false),
    ("cp949", "CP949", false),
    ("cp950", "CP950", false),
    ("cp951", "CP951", false),
    ("cswindows31j", "Windows-31J", false),
    ("ebcdic-cp-us", "IBM037", true),
    ("emacs-mule", "Emacs-Mule", false),
    ("euc-cn", "GB2312", false),
    ("euc-jis-2004", "EUC-JIS-2004", false),
    ("euc-jisx0213", "EUC-JIS-2004", false),
    ("euc-jp", "EUC-JP", false),
    ("euc-jp-ms", "eucJP-ms", false),
    ("euc-kr", "EUC-KR", false),
    ("euc-tw", "EUC-TW", false),
    ("euccn", "GB2312", false),
    ("eucjp", "EUC-JP", false),
    ("eucjp-ms", "eucJP-ms", false),
    ("euckr", "EUC-KR", false),
    ("euctw", "EUC-TW", false),
    ("gb12345", "GB12345", false),
    ("gb18030", "GB18030", false),
    ("gb1988", "GB1988", false),
    ("gb2312", "GB2312", false),
    ("gbk", "GBK", false),
    ("ibm037", "IBM037", true),
    ("ibm437", "IBM437", false),
    ("ibm720", "IBM720", false),
    ("ibm737", "IBM737", false),
    ("ibm775", "IBM775", false),
    ("ibm850", "CP850", false),
    ("ibm852", "IBM852", false),
    ("ibm855", "IBM855", false),
    ("ibm857", "IBM857", false),
    ("ibm860", "IBM860", false),
    ("ibm861", "IBM861", false),
    ("ibm862", "IBM862", false),
    ("ibm863", "IBM863", false),
    ("ibm864", "IBM864", false),
    ("ibm865", "IBM865", false),
    ("ibm866", "IBM866", false),
    ("ibm869", "IBM869", false),
    ("iso-2022-jp", "ISO-2022-JP", true),
    ("iso-2022-jp-2", "ISO-2022-JP-2", true),
    ("iso-2022-jp-kddi", "ISO-2022-JP-KDDI", true),
    ("iso-8859-1", "ISO-8859-1", false),
    ("iso-8859-10", "ISO-8859-10", false),
    ("iso-8859-11", "ISO-8859-11", false),
    ("iso-8859-13", "ISO-8859-13", false),
    ("iso-8859-14", "ISO-8859-14", false),
    ("iso-8859-15", "ISO-8859-15", false),
    ("iso-8859-16", "ISO-8859-16", false),
    ("iso-8859-2", "ISO-8859-2", false),
    ("iso-8859-3", "ISO-8859-3", false),
    ("iso-8859-4", "ISO-8859-4", false),
    ("iso-8859-5", "ISO-8859-5", false),
    ("iso-8859-6", "ISO-8859-6", false),
    ("iso-8859-7", "ISO-8859-7", false),
    ("iso-8859-8", "ISO-8859-8", false),
    ("iso-8859-9", "ISO-8859-9", false),
    ("iso2022-jp", "ISO-2022-JP", true),
    ("iso2022-jp2", "ISO-2022-JP-2", true),
    ("iso8859-1", "ISO-8859-1", false),
    ("iso8859-10", "ISO-8859-10", false),
    ("iso8859-11", "ISO-8859-11", false),
    ("iso8859-13", "ISO-8859-13", false),
    ("iso8859-14", "ISO-8859-14", false),
    ("iso8859-15", "ISO-8859-15", false),
    ("iso8859-16", "ISO-8859-16", false),
    ("iso8859-2", "ISO-8859-2", false),
    ("iso8859-3", "ISO-8859-3", false),
    ("iso8859-4", "ISO-8859-4", false),
    ("iso8859-5", "ISO-8859-5", false),
    ("iso8859-6", "ISO-8859-6", false),
    ("iso8859-7", "ISO-8859-7", false),
    ("iso8859-8", "ISO-8859-8", false),
    ("iso8859-9", "ISO-8859-9", false),
    ("koi8-r", "KOI8-R", false),
    ("koi8-u", "KOI8-U", false),
    ("maccenteuro", "macCentEuro", false),
    ("maccroatian", "macCroatian", false),
    ("maccyrillic", "macCyrillic", false),
    ("macgreek", "macGreek", false),
    ("maciceland", "macIceland", false),
    ("macjapan", "MacJapanese", false),
    ("macjapanese", "MacJapanese", false),
    ("macroman", "macRoman", false),
    ("macromania", "macRomania", false),
    ("macthai", "macThai", false),
    ("macturkish", "macTurkish", false),
    ("macukraine", "macUkraine", false),
    ("pck", "Windows-31J", false),
    ("shift_jis", "Shift_JIS", false),
    ("sjis", "Windows-31J", false),
    ("sjis-docomo", "SJIS-DoCoMo", false),
    ("sjis-kddi", "SJIS-KDDI", false),
    ("sjis-softbank", "SJIS-SoftBank", false),
    ("stateless-iso-2022-jp", "stateless-ISO-2022-JP", false),
    (
        "stateless-iso-2022-jp-kddi",
        "stateless-ISO-2022-JP-KDDI",
        false,
    ),
    ("tis-620", "TIS-620", false),
    ("ucs-2be", "UTF-16BE", false),
    ("ucs-4be", "UTF-32BE", false),
    ("ucs-4le", "UTF-32LE", false),
    ("us-ascii", "US-ASCII", false),
    ("utf-16", "UTF-16", true),
    ("utf-16be", "UTF-16BE", false),
    ("utf-16le", "UTF-16LE", false),
    ("utf-32", "UTF-32", true),
    ("utf-32be", "UTF-32BE", false),
    ("utf-32le", "UTF-32LE", false),
    ("utf-7", "UTF-7", true),
    ("utf-8", "UTF-8", false),
    ("utf-8-hfs", "UTF8-MAC", false),
    ("utf-8-mac", "UTF8-MAC", false),
    ("utf8-docomo", "UTF8-DoCoMo", false),
    ("utf8-kddi", "UTF8-KDDI", false),
    ("utf8-mac", "UTF8-MAC", false),
    ("utf8-softbank", "UTF8-SoftBank", false),
    ("windows-1250", "Windows-1250", false),
    ("windows-1251", "Windows-1251", false),
    ("windows-1252", "Windows-1252", false),
    ("windows-1253", "Windows-1253", false),
    ("windows-1254", "Windows-1254", false),
    ("windows-1255", "Windows-1255", false),
    ("windows-1256", "Windows-1256", false),
    ("windows-1257", "Windows-1257", false),
    ("windows-1258", "Windows-1258", false),
    ("windows-31j", "Windows-31J", false),
    ("windows-874", "Windows-874", false),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_encodings_are_sorted() {
        assert!(RUBY_ENCODINGS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn recognizes_magic_comments() {
        assert_eq!(recognize_encoding(b"x = 1\n"), Ok(None));
        assert_eq!(
            recognize_encoding(b"# encoding: BINARY\n"),
            Ok(Some(SourceEncoding::Binary))
        );
        assert_eq!(
            recognize_encoding(b"#!/usr/bin/env ruby\n# -*- coding: utf-8-unix -*-\n"),
            Ok(Some(SourceEncoding::Utf8))
        );
        assert_eq!(
            recognize_encoding(b"# coding: ISO-8859-15\r\n"),
            Ok(Some(SourceEncoding::Other(
                encoding_rs::ISO_8859_15,
                "ISO-8859-15"
            )))
        );
        assert_eq!(recognize_encoding(b"x = 1 # encoding: binary\n"), Ok(None));
        assert_eq!(
            recognize_encoding(b"# encoding: macRoman\n"),
            Ok(Some(SourceEncoding::Unsupported))
        );
        assert_eq!(
            recognize_encoding(b"# encoding: bogus-enc\n"),
            Err("unknown encoding name - bogus-enc".to_owned())
        );
    }

    #[test]
    fn binary_round_trips_byte_per_character() {
        let source = b"# encoding: BINARY\n%W[\xC0 \"foo\"]\n";
        let (text, encoding) = decode(source).unwrap();
        assert_eq!(text.chars().count(), source.len());
        assert_eq!(encode(&text, encoding), source.to_vec());
    }

    #[test]
    fn other_encodings_are_transcoded() {
        let (text, _) = decode(b"# coding: ISO-8859-15\n# Euro symbol: \xa4\n").unwrap();
        assert_eq!(text, "# coding: ISO-8859-15\n# Euro symbol: \u{20ac}\n");
        let (text, _) = decode(b"# encoding: ISO-8859-1\nx = 'caf\xe9'\n").unwrap();
        assert_eq!(text, "# encoding: ISO-8859-1\nx = 'caf\u{e9}'\n");
        let (text, _) = decode(b"# encoding: Shift_JIS\nx = \"\x82\xa0\"\n").unwrap();
        assert_eq!(text, "# encoding: Shift_JIS\nx = \"\u{3042}\"\n");
    }

    // The messages RuboCop 1.91.0 (Ruby 3.3) reports, before Lint/Syntax's
    // `beautify_message`.
    #[test]
    fn invalid_bytes_are_errors() {
        let error = |bytes: &[u8]| decode(bytes).unwrap_err();
        assert_eq!(
            error(b"x = \"abc\"\ny = \xff\n"),
            "invalid byte sequence in UTF-8"
        );
        assert_eq!(error(b"x = '\xe2\x82'\n"), "invalid byte sequence in UTF-8");
        assert_eq!(
            error(b"# encoding: utf-8\nx = 'a\xffb'\n"),
            "invalid byte sequence in UTF-8"
        );
        assert_eq!(
            error(b"\xef\xbb\xbfx = \"\xff\"\n"),
            "invalid byte sequence in UTF-8"
        );
        assert_eq!(
            error(b"# encoding: us-ascii\nx = \"caf\xc3\xa9\"\n"),
            "\"\\xC3\" on US-ASCII"
        );
        assert_eq!(
            error(b"# encoding: Shift_JIS\nx = \"\x82\xa0\"\ny = \"\xff\xfe\"\n"),
            "\"\\xFF\" on Shift_JIS"
        );
        assert_eq!(
            error(b"# encoding: sjis\nx = \"\xff\"\n"),
            "\"\\xFF\" on Windows-31J"
        );
        assert_eq!(
            error(b"#!/usr/bin/env ruby\n# encoding: Shift_JIS\nx = \"\x81\"\n"),
            "\"\\x81\" followed by \"\\\"\" on Shift_JIS"
        );
        assert_eq!(
            error(b"# encoding: EUC-JP\nx = \"\xa4\xa2\x8f\"\n"),
            "\"\\x8F\" followed by \"\\\"\" on EUC-JP"
        );
        assert_eq!(
            error(b"# encoding: EUC-JP\nx = 1 # \xa4"),
            "incomplete \"\\xA4\" on EUC-JP"
        );
        assert_eq!(
            error(b"# encoding: windows-1253\nx = \"\xaa\"\n"),
            "\"\\xAA\" to UTF-8 in conversion from Windows-1253 to UTF-8"
        );
        assert_eq!(
            error(b"# encoding: bogus-enc\nx = \"a\"\n"),
            "unknown encoding name - bogus-enc"
        );
    }

    #[test]
    fn encodings_encoding_rs_lacks_are_decoded_leniently() {
        assert!(decode(b"# encoding: macRoman\nx = \"\xa5\"\n").is_ok());
        assert!(decode(b"# encoding: CP932\nx = \"\x82\xa0\"\n").is_ok());
    }
}
