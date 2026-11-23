//! `work_queue.WorkQueueService` — family P's handler trait and adapter, re-exported from
//! [`tddy_work_queue::rpc`], which defines them.
//!
//! Kept so the historical paths (`tddy_session_lifecycle::work_queue_rpc::…` and the crate-root
//! re-exports) still resolve.

pub use tddy_work_queue::rpc::{build_pr_stack_entry, WorkQueueHandler, WorkQueueServiceImpl};
