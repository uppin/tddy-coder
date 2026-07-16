//! Side-table mapping task IDs to live PTY master handles for resize/SIGWINCH.
//!
//! The registry itself is transport-agnostic and lives in [`tddy_pty`]; re-exported here so the
//! daemon's existing `crate::pty_registry::{PtyRegistry, PtyControl}` import paths keep working.

pub use tddy_pty::{PtyControl, PtyRegistry};
