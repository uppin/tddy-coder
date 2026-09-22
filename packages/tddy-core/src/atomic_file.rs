//! Atomic file writes live in [`tddy_session_store::atomic_file`]. Re-exported at its old path for
//! existing callers; new code should name `tddy_session_store::atomic_file` directly.

pub use tddy_session_store::atomic_file::*;
