//! Crash-safe file replacement, re-exported at its old path.
//!
//! The implementation moved to the filesystem port ([`tddy_fs`]); this module keeps the path
//! every existing caller uses compiling. New code names `tddy_fs` directly, and code under test
//! takes an [`tddy_fs::Fs`] instead.

pub use tddy_fs::{write_atomic, write_atomic_labelled, write_atomic_with_mode};
