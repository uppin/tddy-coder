//! `tddy-task` — long-running background Task abstraction.
//!
//! Provides:
//! - [`TaskId`], [`TaskStatus`], [`ChannelKind`], [`TaskChannel`], [`TaskHandle`]
//! - [`TaskBody`] / [`TaskContext`] for implementing cancellable task bodies
//! - [`TaskRegistry`] — register, list, spawn, and cancel tasks

pub mod registry;
pub mod task;

pub use registry::{TaskRegistry, TaskRegistryEvent};
pub use task::{ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskId, TaskStatus};
