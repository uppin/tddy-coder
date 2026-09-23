//! `pr_stack.PrStackService` — family P's handler trait and adapter, re-exported from
//! [`tddy_pr_stack::rpc`], which defines them.
//!
//! Kept so the historical paths (`tddy_session_lifecycle::pr_stack_rpc::…` and the crate-root
//! re-exports) still resolve.

pub use tddy_pr_stack::rpc::{build_pr_stack_entry, PrStackHandler, PrStackServiceImpl};
