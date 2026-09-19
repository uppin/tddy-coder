//! What an agent CLI's stream reports as it runs.
//!
//! A plain event type with no dependency on the NDJSON parsing that emits it, so a consumer can
//! name a step's progress without naming the stream it came from.

/// Progress event for real-time display. Each variant has a distinct display representation.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Direct tool use (Read, Bash, Glob, etc.) with optional detail from input.
    ToolUse {
        name: String,
        detail: Option<String>,
        /// Full tool input as a JSON string, when the parser has it (`None` otherwise). Carried
        /// so a host can persist the agent's own tool calls (see `agent_activity`).
        input_json: Option<String>,
        /// The stream's tool-call id, when known. A later [`ProgressEvent::ToolResult`] carrying
        /// the same id is the completion of this call.
        call_id: Option<String>,
    },
    /// Result of a previously-emitted [`ProgressEvent::ToolUse`], correlated by `call_id`.
    ToolResult {
        call_id: String,
        result_json: String,
        is_error: bool,
    },
    /// Sub-agent task started.
    TaskStarted { description: String },
    /// Sub-agent task progress (e.g. "Running find...", "Reading file").
    TaskProgress {
        description: String,
        last_tool: Option<String>,
    },
    /// Agent session started; session_id from first system/init stream event.
    SessionStarted { session_id: String },
    /// Agent process exited; goal identifies the step (e.g. "plan", "acceptance-tests").
    AgentExited { exit_code: i32, goal: String },
}
