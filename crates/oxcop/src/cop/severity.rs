//! Port of `lib/rubocop/cop/severity.rb`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Refactor,
    Convention,
    Warning,
    Error,
    Fatal,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Refactor => "refactor",
            Self::Convention => "convention",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    pub fn code(self) -> char {
        match self {
            Self::Info => 'I',
            Self::Refactor => 'R',
            Self::Convention => 'C',
            Self::Warning => 'W',
            Self::Error => 'E',
            Self::Fatal => 'F',
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "info" => Self::Info,
            "refactor" => Self::Refactor,
            "convention" => Self::Convention,
            "warning" => Self::Warning,
            "error" => Self::Error,
            "fatal" => Self::Fatal,
            _ => return None,
        })
    }
}
