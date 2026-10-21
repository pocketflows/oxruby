//! Port of `lib/rubocop/cli.rb` and `cli/command/execute_runner.rb`.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};

use clap::Parser;

use crate::cop::base::CopOptions;
use crate::cop::registry;
use crate::formatter::{self, FormatterOptions};
use crate::options::{AutocorrectMode, Options};
use crate::runner::Runner;
use crate::version::VERSION;

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_OFFENSES: i32 = 1;
pub const STATUS_ERROR: i32 = 2;

pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let options = match Options::try_parse_from(args) {
        Ok(options) => options,
        Err(e) if !e.use_stderr() => {
            let _ = e.print();
            return STATUS_SUCCESS;
        }
        Err(e) => {
            let _ = e.print();
            return STATUS_ERROR;
        }
    };

    if options.version {
        println!("{VERSION}");
        return STATUS_SUCCESS;
    }
    if options.dump_ast || options.dump_tokens {
        return dump_ast(&options);
    }
    if options.show_ported_cops {
        for name in registry::names() {
            println!("{name}");
        }
        return STATUS_SUCCESS;
    }

    match execute_runner(&options) {
        Ok(status) => status,
        Err(message) => {
            eprintln!("Error: {message}");
            STATUS_ERROR
        }
    }
}

fn execute_runner(options: &Options) -> Result<i32, String> {
    let format = options
        .format
        .clone()
        .unwrap_or_else(|| "progress".to_owned());
    let to_stderr = options.stderr;
    let color = if options.no_color {
        false
    } else if options.color {
        true
    } else if to_stderr {
        std::io::stderr().is_terminal()
    } else {
        std::io::stdout().is_terminal()
    };

    let cop_options = CopOptions {
        autocorrect: options.autocorrect_mode(),
        display_cop_names: options.display_cop_names(),
        json_format: matches!(format.as_str(), "json" | "j"),
        ignore_disable_comments: false,
    };
    let mut runner = Runner::new(options, cop_options);

    if options.list_target_files {
        for file in runner.find_target_files().map_err(|e| e.to_string())? {
            println!("{}", crate::path_util::smart_path(&file));
        }
        return Ok(STATUS_SUCCESS);
    }

    let output: Box<dyn Write> = if to_stderr {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    let formatter_options = FormatterOptions {
        color,
        safe_autocorrect: options.autocorrect_mode() == AutocorrectMode::Safe,
    };
    let mut formatter = formatter::build(&format, output, formatter_options)?;

    let all_passed = runner.run(formatter.as_mut()).map_err(|e| e.to_string())?;
    drop(formatter);

    maybe_print_corrected_source(options, &format, runner.corrected_stdin.as_deref());

    for error in &runner.errors {
        eprintln!("{error}");
    }
    Ok(if !runner.errors.is_empty() {
        STATUS_ERROR
    } else if all_passed {
        STATUS_SUCCESS
    } else {
        STATUS_OFFENSES
    })
}

fn maybe_print_corrected_source(options: &Options, format: &str, corrected: Option<&str>) {
    if formatter::INTEGRATION_FORMATTERS.contains(&format) {
        return;
    }
    if options.stdin.is_none() || options.autocorrect_mode() == AutocorrectMode::Off {
        return;
    }
    let separator = "=".repeat(20);
    if options.stderr {
        eprintln!("{separator}");
    } else {
        println!("{separator}");
    }
    print!("{}", corrected.unwrap_or_default());
    let _ = std::io::stdout().flush();
}

fn dump_ast(options: &Options) -> i32 {
    let mut out = std::io::stdout().lock();
    for path in &options.paths {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return STATUS_ERROR;
            }
        };
        let source = String::from_utf8_lossy(&bytes);
        let buffer = crate::processed_source::normalize_newlines(&source);
        let processed = crate::processed_source::ProcessedSource::new(
            &source,
            &buffer,
            options.dump_ast_version,
            path,
        );
        let _ = writeln!(out, "== {}", path.display());
        if processed.valid_syntax() && options.dump_tokens {
            for token in processed.tokens() {
                let _ = writeln!(
                    out,
                    "{} {} {}..{}",
                    token.ty,
                    escape_text(&token.text),
                    token.range.begin_pos,
                    token.range.end_pos
                );
            }
        } else if processed.valid_syntax() {
            let _ = write!(out, "{}", processed.tree().dump());
        } else {
            let _ = writeln!(out, "syntax error");
        }
    }
    STATUS_SUCCESS
}

fn escape_text(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        if (0x20..=0x7e).contains(&byte) && byte != b'"' && byte != b'\\' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("\\x{byte:02X}"));
        }
    }
    out.push('"');
    out
}
