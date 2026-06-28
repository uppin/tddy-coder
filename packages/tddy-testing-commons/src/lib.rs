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
pub mod sandbox_fixtures;
pub mod sandbox_session_channel;

// Root-level re-exports for ergonomic imports in test files.
pub use builders::{
    a_changeset, a_session_metadata, an_invoke_request, an_invoke_response, ChangesetBuilder,
    InvokeResponseBuilder, SessionMetadataBuilder,
};
pub use fakes::{
    mock_backend, mock_backend_returning_ok, mock_backend_returning_outputs, MockBackend,
};
pub use fs::{tddy_test_home, temp_dir_with_git_repo, temp_session_dir, write_session_yaml};
pub use sandbox_fixtures::{
    process_is_alive, write_connect_proxy_claude_script, write_egress_probe_claude_script,
    CONNECT_PROBE_TUNNEL_OK, EGRESS_PROBE_DIRECT_DENIED, EGRESS_PROBE_SESSION_CHANNEL_OK,
};
pub use sandbox_session_channel::SandboxSessionChannelHost;

/// Convenience re-export so converted tests have one path for pretty diffs.
pub use pretty_assertions::assert_eq;
pub use pretty_assertions::assert_ne;
pub use pretty_assertions::assert_str_eq;
