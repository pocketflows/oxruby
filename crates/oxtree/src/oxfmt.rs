//! Formatting of embedded JavaScript, CSS and JSON the way
//! `oxfmt --stdin-filepath embed.<ext>` does (oxfmt 0.72.0), run with the
//! project root as the working directory so its `.oxfmtrc.json` applies.
//!
//! Two backends: the `oxfmt` executable (what the app's rake task runs), and,
//! with the `oxc` feature, the oxc formatter crates linked in (see `oxc.rs`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Js,
    Css,
    Json,
}

impl Language {
    pub fn extension(self) -> &'static str {
        match self {
            Language::Js => "js",
            Language::Css => "css",
            Language::Json => "json",
        }
    }
}

#[derive(Debug)]
pub enum EmbedError {
    /// `ErbEmbeddedFormatter::OxfmtMissingError`
    Missing(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The linked-in oxc formatter, falling back to the executable for the
    /// few inputs whose output depends on oxfmt's JavaScript side.
    Library,
    /// Always run the `oxfmt` executable.
    Process,
}

thread_local! {
    static TAILWIND_UNSORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a `format` call on this thread, since the last call to this
/// function, left Tailwind classes unsorted because the oxfmt executable
/// isn't installed. Resets the flag.
pub fn take_tailwind_unsorted() -> bool {
    TAILWIND_UNSORTED.with(|flag| flag.replace(false))
}

pub struct Oxfmt {
    root: PathBuf,
    binary: PathBuf,
    #[cfg_attr(not(feature = "oxc"), allow(dead_code))]
    backend: Backend,
    #[cfg(feature = "oxc")]
    library: Option<crate::oxc::Config>,
}

impl Oxfmt {
    /// `binary` defaults to `<root>/node_modules/.bin/oxfmt`, as in the app.
    pub fn new(root: &Path, binary: Option<PathBuf>, backend: Backend) -> Self {
        let binary = binary.unwrap_or_else(|| root.join("node_modules/.bin/oxfmt"));
        #[cfg(feature = "oxc")]
        let library = if backend == Backend::Library {
            crate::oxc::Config::load(root)
        } else {
            None
        };
        Oxfmt {
            root: root.to_path_buf(),
            binary,
            backend,
            #[cfg(feature = "oxc")]
            library,
        }
    }

    pub fn backend(&self) -> Backend {
        #[cfg(feature = "oxc")]
        if self.backend == Backend::Library && self.library.is_some() {
            return Backend::Library;
        }
        Backend::Process
    }

    /// The formatted code, `None` when oxfmt rejects it (non-zero exit).
    pub fn format(&self, code: &str, language: Language) -> Result<Option<String>, EmbedError> {
        #[cfg(feature = "oxc")]
        if self.backend == Backend::Library
            && let Some(config) = &self.library
        {
            match crate::oxc::format(config, code, language) {
                crate::oxc::Outcome::Formatted(out) => return Ok(Some(out)),
                crate::oxc::Outcome::Error => return Ok(None),
                crate::oxc::Outcome::NeedsOxfmt => {}
                // Without the executable, Tailwind classes stay unsorted and
                // the caller is told (user decision: warn, don't fail).
                crate::oxc::Outcome::NeedsTailwindSort(unsorted) => {
                    if !self.binary.exists() {
                        TAILWIND_UNSORTED.with(|flag| flag.set(true));
                        return Ok(Some(unsorted));
                    }
                }
            }
        }
        self.run_process(code, language)
    }

    fn run_process(&self, code: &str, language: Language) -> Result<Option<String>, EmbedError> {
        let missing = || {
            EmbedError::Missing(format!(
                "{} not found; run `npm ci` before formatting ERB",
                self.binary.display()
            ))
        };
        let mut child = Command::new(&self.binary)
            .arg("--stdin-filepath")
            .arg(format!("embed.{}", language.extension()))
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| missing())?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(code.as_bytes());
        }
        let output = child.wait_with_output().map_err(|_| missing())?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(all(test, feature = "oxc"))]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxtree-oxfmt-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".oxfmtrc.json"),
            r#"{"experimentalTailwindcss": {"functions": ["cn"]}, "printWidth": 100}"#,
        )
        .unwrap();
        dir
    }

    // User decision: a Tailwind input without the oxfmt executable is
    // formatted by the library with its classes unsorted, and flagged.
    #[test]
    fn tailwind_without_oxfmt_is_unsorted_with_a_flag() {
        let dir = root("tw");
        let oxfmt = Oxfmt::new(&dir, Some(dir.join("missing-oxfmt")), Backend::Library);
        assert_eq!(oxfmt.backend(), Backend::Library);
        take_tailwind_unsorted();
        let out = oxfmt
            .format("const a = cn( \"p-4 flex\" )\n", Language::Js)
            .unwrap();
        assert_eq!(out.as_deref(), Some("const a = cn(\"p-4 flex\");\n"));
        assert!(take_tailwind_unsorted());
        assert!(!take_tailwind_unsorted());

        let out = oxfmt.format("const b = 1\n", Language::Js).unwrap();
        assert_eq!(out.as_deref(), Some("const b = 1;\n"));
        assert!(!take_tailwind_unsorted());

        // An oxfmt that is present but can't run is still an error.
        let broken = dir.join("broken-oxfmt");
        std::fs::write(&broken, "not a program").unwrap();
        let present = Oxfmt::new(&dir, Some(broken), Backend::Library);
        assert!(
            present
                .format("const a = cn( \"p-4 flex\" )\n", Language::Js)
                .is_err()
        );
        assert!(!take_tailwind_unsorted());

        // The executable backend still requires the executable.
        let process = Oxfmt::new(&dir, Some(dir.join("missing-oxfmt")), Backend::Process);
        assert!(process.format("const b = 1\n", Language::Js).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
