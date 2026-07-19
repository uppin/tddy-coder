//! Transport- and host-agnostic PTY core shared by `tddy-daemon` and `tddy-coder`.
//!
//! Provides the PTY spawn spec, the I/O pump that plumbs a `portable_pty` master through a
//! [`tddy_task::Task`], and the master registry used for resize/SIGWINCH. Host-specific concerns —
//! OS-user impersonation, privilege drops, per-user `PATH`/`HOME` — live in the daemon, which
//! pre-computes the final `argv`/`env` and passes them in via [`PtySpawnSpec`]. This crate has no
//! notion of an `os_user`.

pub mod registry;
pub mod runtime;

pub use bytes::{self, Bytes};
pub use registry::{PtyControl, PtyRegistry};
pub use runtime::{PtyReady, PtyRuntime, PtySpawnSpec, DEFAULT_TERM_COLS, DEFAULT_TERM_ROWS};
