//! Generates the cop registry: every `src/cop/<department>/<name>.rs` file
//! defines a cop struct named after the file in CamelCase (and implementing
//! `Default` and `Cop`). Helpers shared by cops live elsewhere (`cop/mixin/`
//! or subdirectories), so adding a cop never touches a shared file.

use std::fmt::Write as _;
use std::path::Path;

const DEPARTMENTS: &[&str] = &[
    "bundler",
    "gemspec",
    "layout",
    "lint",
    "metrics",
    "migration",
    "naming",
    "security",
    "style",
];

fn camel_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let cop_dir = Path::new(&manifest_dir).join("src").join("cop");
    println!("cargo:rerun-if-changed={}", cop_dir.display());

    let mut modules = String::new();
    let mut constructors = String::new();
    for department in DEPARTMENTS {
        let dir = cop_dir.join(department);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "rs"))
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    .filter(|stem| stem != "mod")
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let _ = writeln!(modules, "pub mod {department} {{");
        for file in &files {
            let path = dir.join(format!("{file}.rs"));
            let _ = writeln!(modules, "    #[path = {:?}]", path.display().to_string());
            let _ = writeln!(modules, "    mod {file};");
            let _ = writeln!(modules, "    pub use {file}::{};", camel_case(file));
            let _ = writeln!(
                constructors,
                "    || Box::new({department}::{}::default()),",
                camel_case(file)
            );
        }
        let _ = writeln!(modules, "}}");
    }

    let out = format!(
        "{modules}\n/// Constructors of every ported cop.\n#[allow(clippy::default_constructed_unit_structs)]\npub const CONSTRUCTORS: &[fn() -> Box<dyn crate::cop::Cop>] = &[\n{constructors}];\n"
    );
    let out_path = Path::new(&std::env::var("OUT_DIR").unwrap()).join("cops.rs");
    std::fs::write(out_path, out).unwrap();
}
