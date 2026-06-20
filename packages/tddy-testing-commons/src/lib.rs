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

// Root-level re-exports for ergonomic imports in test files.
pub use builders::{
    a_changeset, a_session_metadata, an_invoke_request, an_invoke_response, ChangesetBuilder,
    InvokeResponseBuilder, SessionMetadataBuilder,
};
pub use fakes::{mock_backend, mock_backend_returning_ok, mock_backend_returning_outputs, MockBackend};
pub use fs::{temp_dir_with_git_repo, temp_session_dir, write_session_yaml};

/// Convenience re-export so converted tests have one path for pretty diffs.
pub use pretty_assertions::assert_eq;
pub use pretty_assertions::assert_ne;
pub use pretty_assertions::assert_str_eq;
