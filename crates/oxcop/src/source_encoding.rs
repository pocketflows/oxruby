//! How source bytes become the UTF-8 text cops inspect, and back: a port of
//! `Parser::Source::Buffer.recognize_encoding` and `reencode_string`, with
//! rubocop-ast's default of UTF-8 (`ProcessedSource#initialize`).
//!
//! - No magic encoding comment (or a UTF-8 one): the bytes are UTF-8.
//! - `# encoding: BINARY` (`ASCII-8BIT`): Ruby keeps a binary string, where
//!   every byte is one character. Each byte becomes the character U+00XX, so
//!   reported columns and lengths count bytes as RuboCop's do, and
//!   [`encode`] turns the characters back into those bytes.
//! - Any other encoding: transcoded to UTF-8 (`encode(Encoding::UTF_8)`), as
//!   the buffer is; corrections are written as UTF-8, as RuboCop writes them.

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
    Other(&'static encoding_rs::Encoding),
}

/// `Parser::Source::Buffer::ENCODING_RE`.
static ENCODING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[\s#](?:en)?coding\s*[:=]\s*(?:(utf8-mac)|([A-Za-z0-9_-]+?)(?:-unix|-dos|-mac)|([A-Za-z0-9_-]+))",
    )
    .expect("valid regex")
});

/// `Buffer.recognize_encoding`: the encoding a magic comment (or a BOM)
/// names on the first line, or the second after a shebang.
pub fn recognize_encoding(source: &[u8]) -> Option<SourceEncoding> {
    if source.is_empty() {
        return None;
    }
    let mut lines = source.splitn(3, |&b| b == b'\n');
    let first_line = lines.next().unwrap_or_default();
    let second_line = lines.next();
    if first_line.starts_with(b"\xEF\xBB\xBF") {
        return Some(SourceEncoding::Utf8);
    }
    let encoding_line = if first_line.starts_with(b"#!") {
        second_line?
    } else {
        first_line
    };
    if encoding_line.first() != Some(&b'#') {
        return None;
    }
    let captures = ENCODING_RE.captures(encoding_line)?;
    let name = captures
        .get(1)
        .or_else(|| captures.get(2))
        .or_else(|| captures.get(3))?;
    find_encoding(&String::from_utf8_lossy(name.as_bytes()))
}

/// `Encoding.find`, for the encodings encoding_rs knows. Unknown names are
/// left as UTF-8.
fn find_encoding(name: &str) -> Option<SourceEncoding> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "binary" | "ascii-8bit" => SourceEncoding::Binary,
        "utf-8" | "utf8" | "utf8-mac" | "utf-8-mac" | "us-ascii" | "ascii" => SourceEncoding::Utf8,
        "iso-8859-1" | "iso8859-1" | "latin1" => SourceEncoding::Latin1,
        _ => match encoding_rs::Encoding::for_label(lower.as_bytes()) {
            Some(encoding) if encoding == encoding_rs::UTF_8 => SourceEncoding::Utf8,
            Some(encoding) => SourceEncoding::Other(encoding),
            None => return None,
        },
    })
}

/// The text cops see for `bytes`, and its encoding (for [`encode`]).
/// Bytes that aren't valid in their encoding are replaced (RuboCop reports
/// those files as unparsable instead).
pub fn decode(bytes: &[u8]) -> (String, SourceEncoding) {
    let encoding = recognize_encoding(bytes).unwrap_or_default();
    let text = match encoding {
        SourceEncoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        SourceEncoding::Binary | SourceEncoding::Latin1 => {
            bytes.iter().map(|&b| char::from(b)).collect()
        }
        SourceEncoding::Other(encoding) => {
            encoding.decode_without_bom_handling(bytes).0.into_owned()
        }
    };
    (text, encoding)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_magic_comments() {
        assert_eq!(recognize_encoding(b"x = 1\n"), None);
        assert_eq!(
            recognize_encoding(b"# encoding: BINARY\n"),
            Some(SourceEncoding::Binary)
        );
        assert_eq!(
            recognize_encoding(b"#!/usr/bin/env ruby\n# -*- coding: utf-8-unix -*-\n"),
            Some(SourceEncoding::Utf8)
        );
        assert_eq!(
            recognize_encoding(b"# coding: ISO-8859-15\r\n"),
            Some(SourceEncoding::Other(encoding_rs::ISO_8859_15))
        );
        assert_eq!(recognize_encoding(b"x = 1 # encoding: binary\n"), None);
    }

    #[test]
    fn binary_round_trips_byte_per_character() {
        let source = b"# encoding: BINARY\n%W[\xC0 \"foo\"]\n";
        let (text, encoding) = decode(source);
        assert_eq!(text.chars().count(), source.len());
        assert_eq!(encode(&text, encoding), source.to_vec());
    }

    #[test]
    fn other_encodings_are_transcoded() {
        let (text, _) = decode(b"# coding: ISO-8859-15\n# Euro symbol: \xa4\n");
        assert_eq!(text, "# coding: ISO-8859-15\n# Euro symbol: \u{20ac}\n");
    }
}
