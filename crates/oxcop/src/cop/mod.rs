//! Port of `lib/rubocop/cop/`. Each department lives in its own module,
//! mirroring `lib/rubocop/cop/<department>/`.

pub mod base;
pub mod commissioner;
pub mod corrector;
pub mod layout;
pub mod lint;
pub mod offense;
pub mod registry;
pub mod severity;
pub mod team;

pub use base::{Cop, Investigation};
pub use corrector::Corrector;
pub use offense::Offense;
pub use severity::Severity;
