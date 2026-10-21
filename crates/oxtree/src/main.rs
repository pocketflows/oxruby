#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().collect();
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
        return;
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
        return;
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
        return;
    }
    if args.len() == 3 && args[1] == "--format-erb" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        match oxtree::erb::format(&source, 80) {
            Ok(out) => print!("{out}"),
            Err(e) => {
                eprintln!("{e} ({}:{})", e.lineno, e.column);
                std::process::exit(1);
            }
        }
        return;
    }
    if args.len() == 3 && args[1] == "--format-file" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        match oxtree::format(&source, 80, oxtree::format::Options::default()) {
            Ok(out) => print!("{out}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.len() == 3 && args[1] == "--dump-ast" {
        let source = std::fs::read_to_string(&args[2]).expect("read");
        match oxtree::parser::parse(&source) {
            Ok(ast) => print!("{}", ast.dump()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
}
