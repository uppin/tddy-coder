//! `tddy-task` — long-running background Task abstraction.
//!
//! Provides:
//! - [`TaskId`], [`TaskStatus`], [`ChannelKind`], [`TaskChannel`], [`TaskHandle`]
//! - [`TaskBody`] / [`TaskContext`] for implementing cancellable task bodies
//! - [`TaskRegistry`] — register, list, spawn, and cancel tasks

pub mod idle;
pub mod registry;
pub mod task;

pub use idle::IdleTimeoutTracker;
pub use registry::{TaskRegistry, TaskRegistryEvent};
pub use task::{ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskId, TaskStatus};
