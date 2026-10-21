//! oxfmt 0.72.0's formatting of `embed.js`/`embed.css`/`embed.json` on stdin,
//! done in-process with the oxc formatter crates that oxfmt itself calls
//! (`apps/oxfmt/src/core/format.rs` at tag `oxfmt_v0.72.0`).
//!
//! Only configurations whose effect is fully known are handled here: the
//! core options (`printWidth`, `tabWidth`, `useTabs`, `endOfLine`) and the
//! Tailwind class sorter. Anything else in `.oxfmtrc.json` (or an
//! `.editorconfig`, or a TypeScript config) makes `Config::load` return
//! `None`, and the caller runs the `oxfmt` executable instead. Inputs whose
//! output depends on oxfmt's JavaScript side (Tailwind class sorting,
//! embedded languages in template literals) are detected while formatting
//! and reported as `NeedsOxfmt`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use oxc_allocator::Allocator;
use oxc_formatter::{JsFormatOptions, SortTailwindcssOptions};
use oxc_formatter_core::{
    CoreFormatOptions, DispatchResponse, FormatDispatcher, FormatOptions, FormatSession,
    IndentStyle, IndentWidth, InputKind, LineEnding, LineWidth, SessionServices, TailwindSorter,
};
use oxc_formatter_css::{CssFormatOptions, CssVariant};
use oxc_formatter_json::{JsonFormatOptions, JsonVariant};
use oxc_span::SourceType;

use crate::oxfmt::Language;

#[derive(Debug, Clone)]
pub struct Config {
    core: CoreFormatOptions,
    /// `experimentalTailwindcss.functions` when Tailwind sorting is on.
    tailwind_functions: Option<Vec<String>>,
}

pub enum Outcome {
    Formatted(String),
    /// oxfmt would exit non-zero (the code doesn't parse).
    Error,
    /// The output depends on oxfmt's JavaScript side; run the executable.
    NeedsOxfmt,
    /// Only Tailwind class sorting needs the executable; this is the output
    /// with the classes left as written, used when oxfmt isn't installed.
    NeedsTailwindSort(String),
}

const CONFIG_FILES: &[&str] = &[
    ".oxfmtrc.json",
    ".oxfmtrc.jsonc",
    "oxfmt.config.ts",
    "oxfmt.config.mts",
];

/// Keys that don't change how `embed.<ext>` on stdin is printed.
const NEUTRAL_KEYS: &[&str] = &["$schema", "ignorePatterns"];

impl Config {
    /// Finds the configuration oxfmt would use when run in `root`.
    pub fn load(root: &Path) -> Option<Config> {
        let mut dir = Some(root);
        let mut config_file = None;
        while let Some(d) = dir {
            if d.join(".editorconfig").exists() {
                return None;
            }
            if config_file.is_none() {
                config_file = CONFIG_FILES.iter().map(|f| d.join(f)).find(|p| p.exists());
            }
            dir = d.parent();
        }
        let mut config = Config {
            core: CoreFormatOptions::default(),
            tailwind_functions: None,
        };
        let Some(path) = config_file else {
            return Some(config);
        };
        if path.extension().is_some_and(|e| e == "ts" || e == "mts") {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&strip_json_comments(&text)).ok()?;
        let object = value.as_object()?;
        for (key, value) in object {
            match key.as_str() {
                "useTabs" => {
                    config.core.indent_style = if value.as_bool()? {
                        IndentStyle::Tab
                    } else {
                        IndentStyle::Space
                    };
                }
                "tabWidth" => {
                    let width = u8::try_from(value.as_u64()?).ok()?;
                    config.core.indent_width = IndentWidth::try_from(width).ok()?;
                }
                "printWidth" => {
                    let width = u16::try_from(value.as_u64()?).ok()?;
                    config.core.line_width = LineWidth::try_from(width).ok()?;
                }
                "endOfLine" => {
                    config.core.line_ending = match value.as_str()? {
                        "lf" => LineEnding::Lf,
                        "crlf" => LineEnding::Crlf,
                        "cr" => LineEnding::Cr,
                        _ => return None,
                    };
                }
                "experimentalTailwindcss" | "sortTailwindcss" => {
                    let functions = value
                        .get("functions")
                        .and_then(|f| f.as_array())
                        .map(|f| {
                            f.iter()
                                .filter_map(|s| s.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    config.tailwind_functions = Some(functions);
                }
                key if NEUTRAL_KEYS.contains(&key) => {}
                _ => return None,
            }
        }
        Some(config)
    }
}

/// Removes `//` and `/* */` comments outside of strings (JSONC).
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn format(config: &Config, source: &str, language: Language) -> Outcome {
    // oxfmt prints nothing for whitespace-only input.
    if source.trim().is_empty() {
        return Outcome::Formatted(String::new());
    }
    let allocator = Allocator::default();
    let saw_tailwind = Arc::new(AtomicBool::new(false));
    let saw_embed = Arc::new(AtomicBool::new(false));
    let services = {
        let tailwind = Arc::clone(&saw_tailwind);
        let sorter: TailwindSorter = Arc::new(move |classes: Vec<String>| {
            if !classes.is_empty() {
                tailwind.store(true, Ordering::Relaxed);
            }
            classes
        });
        let embed = Arc::clone(&saw_embed);
        let dispatcher: FormatDispatcher = Arc::new(move |_session, _request| {
            embed.store(true, Ordering::Relaxed);
            Ok(DispatchResponse::PreserveOriginal)
        });
        SessionServices {
            dispatcher: Some(dispatcher),
            string_embedder: None,
            tailwind_sorter: Some(sorter),
        }
    };
    let session = FormatSession::with_services(&allocator, InputKind::PhysicalFile, services);
    let tailwind = config.tailwind_functions.is_some();

    let printed = match language {
        Language::Json => {
            let mut options = JsonFormatOptions {
                variant: JsonVariant::Json,
                ..Default::default()
            };
            options.apply_core(config.core);
            match oxc_formatter_json::format(&allocator, source, options) {
                Ok(formatted) => formatted.print(),
                Err(_) => return Outcome::Error,
            }
        }
        Language::Css => {
            let mut options = CssFormatOptions {
                variant: CssVariant::Css,
                ..Default::default()
            };
            options.apply_core(config.core);
            options.sort_tailwindcss = tailwind;
            match oxc_formatter_css::format_with_session(&session, source, options) {
                Ok(formatted) => formatted.print(),
                Err(_) => return Outcome::Error,
            }
        }
        Language::Js => {
            let Ok(source_type) = SourceType::from_path(Path::new("embed.js")) else {
                return Outcome::Error;
            };
            let mut options = JsFormatOptions::default();
            options.apply_core(config.core);
            if let Some(functions) = &config.tailwind_functions {
                options.sort_tailwindcss = Some(SortTailwindcssOptions {
                    functions: functions.clone(),
                    ..Default::default()
                });
            }
            match oxc_formatter::format_with_session(&session, source, source_type, options) {
                Ok(formatted) => formatted.print(),
                Err(_) => return Outcome::Error,
            }
        }
    };
    if saw_embed.load(Ordering::Relaxed) {
        return Outcome::NeedsOxfmt;
    }
    if saw_tailwind.load(Ordering::Relaxed) {
        return match printed {
            Ok(printed) => Outcome::NeedsTailwindSort(printed.into_code()),
            Err(_) => Outcome::NeedsOxfmt,
        };
    }
    match printed {
        Ok(printed) => Outcome::Formatted(printed.into_code()),
        Err(_) => Outcome::Error,
    }
}
