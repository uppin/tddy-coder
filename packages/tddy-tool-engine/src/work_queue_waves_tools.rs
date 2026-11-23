//! `work_queue_waves` (`#workqueue` 10/14): the queue's waves, derived line, each item's readiness and
//! the line check. Final name.

use crate::catalog::ToolDef;

pub const WORK_QUEUE_WAVES: &str = "work_queue_waves";

/// The waves tool's name, as a list the registries chain with the other tool-name lists.
pub const WORK_QUEUE_WAVES_TOOL_NAMES: [&str; 1] = [WORK_QUEUE_WAVES];

/// `work_queue_waves {}`: read-only, takes no argument (the session's own queue).
pub fn work_queue_waves_tool_def() -> ToolDef {
    ToolDef {
        name: WORK_QUEUE_WAVES.to_string(),
        description:
            "Read the work queue's waves (wave 1 is every item that needs nothing; wave k \
                      every item whose needs are all in lower waves), the derived line (waves \
                      concatenated, blockers first), each item's readiness (the unmet needs in the \
                      wave immediately below it) and whether the current branch line steps back a \
                      wave."
                .to_string(),
        input_schema_json: r#"{"type":"object","properties":{},"additionalProperties":false}"#
            .to_string(),
    }
}
