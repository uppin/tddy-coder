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

/// The `tasks.TaskService` entry the daemon's wiring layer registers.
///
/// `#unbundle` node 3 moved this service out of `tddy-daemon` and into the crate that already owns
/// the long-running task surface this crate already implements. The daemon's whole contract with a subsystem is a
/// [`tddy_rpc::ServiceEntry`], so registration becomes a call to the owner rather than a
/// daemon-internal type.
pub fn build_task_service_entry() -> tddy_rpc::ServiceEntry {
    // TODO(sandbox-spawn-services): implement
    unimplemented!("build_task_service_entry")
}

#[cfg(test)]
mod unbundle_service_entry_tests {
    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        assert_eq!(super::build_task_service_entry().name, "tasks.TaskService");
    }
}
