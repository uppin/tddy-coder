//! Shared test infrastructure for the tddy workspace.
//!
//! Add as a `[dev-dependencies]` entry in any crate that needs test builders, fakes, or
//! assertion helpers:
//!
//! ```toml
//! [dev-dependencies]
//! tddy-testing-commons = { path = "../tddy-testing-commons" }
//! ```

pub mod assertions;
pub mod builders;
pub mod fakes;
pub mod fs;

/// Convenience re-export so converted tests have one path for pretty diffs.
pub use pretty_assertions::assert_eq;
pub use pretty_assertions::assert_ne;
pub use pretty_assertions::assert_str_eq;
