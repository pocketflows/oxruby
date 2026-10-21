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
