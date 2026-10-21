//! The one place oxcop calls Prism's parser, so a different source of
//! tokens or trees could replace it.

use std::cell::RefCell;
use std::collections::HashMap;

pub use ruby_prism::ParseResult;
use ruby_prism::{Options, ParseOptions, Version};

fn version_for(ruby_version: f64) -> Version {
    let tenths = (ruby_version * 10.0).round() as i64;
    match tenths {
        ..=33 => Version::CRuby3_3,
        34 => Version::CRuby3_4,
        35..=40 => Version::CRuby3_5,
        _ => Version::CRuby4_1,
    }
}

thread_local! {
    /// `ParseOptions` must outlive each `ParseResult` borrowing them; there
    /// are only a handful of versions, so each thread keeps one per version.
    static OPTIONS: RefCell<HashMap<i64, &'static ParseOptions>> = RefCell::new(HashMap::new());
}

fn options(ruby_version: f64) -> &'static ParseOptions {
    let key = (ruby_version * 10.0).round() as i64;
    OPTIONS.with(|cache| {
        *cache.borrow_mut().entry(key).or_insert_with(|| {
            Box::leak(Box::new(
                Options::default()
                    .version(version_for(ruby_version))
                    .build(),
            ))
        })
    })
}

/// Parses `source` as the given Ruby version (`TargetRubyVersion`).
pub fn parse(source: &str, ruby_version: f64) -> ParseResult<'_> {
    ruby_prism::parse_with_options(source.as_bytes(), options(ruby_version))
}
