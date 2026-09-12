//! Idle-timeout tracker for relay daemon mode.
//!
//! The implementation now lives in `tddy-task` ([`tddy_task::IdleTimeoutTracker`]) so it can
//! be shared with other long-running subsystems (e.g. the reusable-LSP registry). This
//! re-export keeps the historical `crate::relay_idle::IdleTimeoutTracker` path stable.

pub use tddy_task::IdleTimeoutTracker;
