//! `tddy-task` — long-running background Task abstraction.
//!
//! Provides:
//! - [`TaskId`], [`TaskStatus`], [`ChannelKind`], [`TaskChannel`], [`TaskHandle`]
//! - [`TaskBody`] / [`TaskContext`] for implementing cancellable task bodies
//! - [`TaskRegistry`] — register, list, spawn, and cancel tasks
//! - [`TerminalCapture`] — bounded replay ring that survives eviction of sticky terminal modes

pub mod idle;
pub mod registry;
pub mod task;
pub mod terminal_capture;

pub use idle::IdleTimeoutTracker;
pub use registry::{TaskRegistry, TaskRegistryEvent};
pub use task::{
    AppliedOffset, ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskId, TaskStatus,
};
pub use terminal_capture::{CaptureChunk, TerminalCapture};
