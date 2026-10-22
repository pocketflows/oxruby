#![forbid(unsafe_code)]
fn main() -> std::process::ExitCode {
    oxcop::Oxcop::new().run()
}
