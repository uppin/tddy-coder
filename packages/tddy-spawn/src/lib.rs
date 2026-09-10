//! Spawning a session's process: the unprivileged spawn worker, and delegation to the privileged
//! supervisor when one is brokering.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 3, as a **separate crate from
//! `tddy-daemon-sandbox`** — the two share no `crate::` edge and their dependency sets are disjoint
//! (`tddy-supervisor` here, six `tddy-sandbox*` crates there). Confinement and privileged fork are
//! different concerns.
//!
//! # The fork happens before tokio starts
//!
//! `main.rs` calls [`supervisor_client::spawn_backend_choice`] and then
//! [`supervisor_client::spawn_worker_for`] **before** it builds the tokio runtime. That ordering is
//! not incidental: forking a process that already has a multi-threaded runtime is unsound, because
//! only the calling thread survives the fork and any lock the others held is held forever. Anything
//! moved here has to preserve it, so the worker's constructor stays synchronous and takes no
//! runtime handle.

pub mod spawn_worker;
pub mod spawner;
pub mod supervisor_client;
pub mod supervisor_spawn;
