#![forbid(unsafe_code)]

//! The `oxtree` executable: syntax_tree's `stree` command line plus the
//! `stree-new` mode (see `cli/`). The `--format-*`/`--dump-ast` commands are
//! hidden development helpers used by `tools/reference_*.rb`.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(status) = dev(&args) {
        std::process::exit(status);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut stdin = stdin.lock();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let mut err = stderr.lock();
    let status = {
        let mut io = oxtree::cli::Io {
            stdin: &mut stdin,
            stdout: &mut out,
            stderr: &mut err,
        };
        oxtree::cli::run(&args[1..], &cwd, &mut io)
    };
    let _ = out.flush();
    drop(out);
    std::process::exit(status);
}

/// The hidden development commands; `None` when `args` isn't one.
fn dev(args: &[String]) -> Option<i32> {
    if args.len() == 6 && args[1] == "--format-list" {
        let (root, out, width, list) = (&args[2], &args[3], &args[4], &args[5]);
        let width: usize = width.parse().unwrap();
        for rel in std::fs::read_to_string(list).unwrap().lines() {
            let source = std::fs::read_to_string(format!("{root}/{rel}")).unwrap_or_default();
            let result = match oxtree::format(&source, width, oxtree::format::Options::default()) {
                Ok(out) => out,
                Err(e) => format!("ERROR {e}"),
            };
            std::fs::write(format!("{out}/{}", rel.replace('/', "__")), result).unwrap();
        }
        return Some(0);
    }
    if args.len() == 9 && args[1] == "--format-embedded-list" {
        let (root, out, width, list) = (&args[2], &args[3], &args[4], &args[5]);
        let backend = if args[6] == "process" {
            oxtree::oxfmt::Backend::Process
        } else {
            oxtree::oxfmt::Backend::Library
        };
        let oxfmt = oxtree::oxfmt::Oxfmt::new(
            std::path::Path::new(&args[7]),
            Some(std::path::PathBuf::from(&args[8])),
            backend,
        );
        let width: usize = width.parse().unwrap();
        for rel in std::fs::read_to_string(list).unwrap().lines() {
            let source = std::fs::read_to_string(format!("{root}/{rel}")).unwrap_or_default();
            let result = match oxtree::embedded::format(&source, width, rel, &oxfmt) {
                Ok(out) => out,
                Err(e) => format!("ERROR {}: {}", e.class_name(), e.message()),
            };
            std::fs::write(format!("{out}/{}", rel.replace('/', "__")), result).unwrap();
        }
        return Some(0);
    }
    if args.len() == 6 && args[1] == "--format-erb-list" {
        let (root, out, width, list) = (&args[2], &args[3], &args[4], &args[5]);
        let width: usize = width.parse().unwrap();
        for rel in std::fs::read_to_string(list).unwrap().lines() {
            let source = std::fs::read_to_string(format!("{root}/{rel}")).unwrap_or_default();
            let result = match oxtree::erb::format(&source, width) {
                Ok(out) => out,
                Err(e) => format!("ERROR {} {}:{}", e.message, e.lineno, e.column),
            };
            std::fs::write(format!("{out}/{}", rel.replace('/', "__")), result).unwrap();
        }
        return Some(0);
    }
    if args.len() == 3 && args[1] == "--format-erb" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        return Some(match oxtree::erb::format(&source, 80) {
            Ok(out) => {
                print!("{out}");
                0
            }
            Err(e) => {
                eprintln!("{e} ({}:{})", e.lineno, e.column);
                1
            }
        });
    }
    if args.len() == 3 && args[1] == "--format-file" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        return Some(
            match oxtree::format(&source, 80, oxtree::format::Options::default()) {
                Ok(out) => {
                    print!("{out}");
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            },
        );
    }
    if args.len() == 3 && args[1] == "--dump-ast" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        return Some(match oxtree::parser::parse(&source) {
            Ok(ast) => {
                print!("{}", ast.dump());
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        });
    }
    None
}
