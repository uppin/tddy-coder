//! The PR-stack data model and its git/GitHub operations.
//!
//! Everything here reads and writes a session's `Changeset.stack` (from `tddy-core`), drives `git`
//! (through `tddy-git`), or talks to the GitHub REST API (through `tddy-github`). None of it is a
//! workflow recipe.
//!
//! [`rpc`] is the one transport-facing module: it holds the `work_queue.WorkQueueService` handler trait
//! and adapter, so the session lifecycle can name the trait without depending on the daemon crate
//! that implements it.

pub mod assess;
pub mod docs;
pub mod git_ops;
pub mod inbox_reads;
pub mod internal_status;
pub mod item_ops;
pub mod merge_repoint;
pub mod op_journal;
pub mod lineage;
pub mod pr_insight;
pub mod rpc;

pub use rpc::WORK_QUEUE_SERVICE;
