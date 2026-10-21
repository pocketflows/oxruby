fn main() {
    let args: Vec<String> = std::env::args().collect();
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
