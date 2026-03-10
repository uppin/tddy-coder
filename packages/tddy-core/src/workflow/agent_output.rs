//! Thread-local bridge for agent output and progress sinks.
//!
//! The runner sets sinks before each task; tasks read them when building InvokeRequest.
//! Used so agent output and progress (ToolUse, TaskStarted, TaskProgress) appear in the TUI.

use crate::backend::{AgentOutputSink, ProgressSink};
use std::cell::RefCell;

thread_local! {
    static AGENT_SINK: RefCell<Option<AgentOutputSink>> = RefCell::new(None);
    static PROGRESS_SINK: RefCell<Option<ProgressSink>> = RefCell::new(None);
}

/// Set the agent output sink for the current thread. Call before running a task.
pub fn set_agent_sink(sink: Option<AgentOutputSink>) {
    AGENT_SINK.with(|cell| *cell.borrow_mut() = sink);
}

/// Get the agent output sink for the current thread. Used by tasks when building InvokeRequest.
pub fn get_agent_sink() -> Option<AgentOutputSink> {
    AGENT_SINK.with(|cell| cell.borrow().clone())
}

/// Set the progress sink for the current thread. Call before running a task.
pub fn set_progress_sink(sink: Option<ProgressSink>) {
    PROGRESS_SINK.with(|cell| *cell.borrow_mut() = sink);
}

/// Get the progress sink for the current thread. Used by tasks when building InvokeRequest.
pub fn get_progress_sink() -> Option<ProgressSink> {
    PROGRESS_SINK.with(|cell| cell.borrow().clone())
}

/// Set both sinks. Convenience for runner.
pub fn set_sinks(agent: Option<AgentOutputSink>, progress: Option<ProgressSink>) {
    set_agent_sink(agent);
    set_progress_sink(progress);
}

/// Clear both sinks. Call after task completes.
pub fn clear_sinks() {
    set_agent_sink(None);
    set_progress_sink(None);
}
