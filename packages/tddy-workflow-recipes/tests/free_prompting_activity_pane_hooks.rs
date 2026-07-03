//! The TUI activity pane shows assistant text from [`WorkflowEvent::AgentOutput`]. [`RunnerHooks::agent_output_sink`]
//! must forward streaming lines from backends (e.g. Cursor) the same way [`tddy_workflow_recipes::tdd::TddWorkflowHooks`] does.
//!
//! It must also forward tool-call activity from [`WorkflowEvent::Progress`] the same way, so
//! backends with no other way to show live activity (e.g. `FastContextBackend`, which has no
//! native CLI stream to parse) still surface `ProgressEvent::ToolUse` in the activity pane.

use std::sync::mpsc;
use std::time::Duration;

use tddy_core::presenter::WorkflowEvent;
use tddy_core::ProgressEvent;
use tddy_workflow_recipes::free_prompting::FreePromptingWorkflowHooks;

#[test]
fn free_prompting_hooks_wire_agent_output_sink_to_workflow_events() {
    // Given
    let (tx, rx) = mpsc::channel::<WorkflowEvent>();
    let hooks = FreePromptingWorkflowHooks::new(Some(tx));
    let sink = hooks
        .agent_output_sink()
        .expect("free-prompting hooks must expose AgentOutputSink so assistant streaming reaches the activity pane");

    // When
    sink.emit("streamed assistant fragment");

    // Then
    let ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("sink emit must deliver WorkflowEvent::AgentOutput to the presenter channel");
    assert!(
        matches!(ev, WorkflowEvent::AgentOutput(ref s) if s == "streamed assistant fragment"),
        "expected AgentOutput with streamed text, got {:?}",
        ev
    );
}

#[test]
fn free_prompting_hooks_wire_progress_sink_to_workflow_events() {
    // Given
    let (tx, rx) = mpsc::channel::<WorkflowEvent>();
    let hooks = FreePromptingWorkflowHooks::new(Some(tx));
    let sink = hooks
        .progress_sink()
        .expect("free-prompting hooks must expose ProgressSink so tool-call activity (e.g. from FastContextBackend) reaches the activity pane");

    // When
    sink.emit(&ProgressEvent::ToolUse {
        name: "GLOB".to_string(),
        detail: Some(r#"{"pattern":"src/**/*.rs"}"#.to_string()),
    });

    // Then
    let ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("sink emit must deliver WorkflowEvent::Progress to the presenter channel");
    match ev {
        WorkflowEvent::Progress(ProgressEvent::ToolUse { name, detail }) => {
            assert_eq!(name, "GLOB", "ToolUse event must name the dispatched tool");
            assert_eq!(
                detail.as_deref(),
                Some(r#"{"pattern":"src/**/*.rs"}"#),
                "ToolUse detail must carry the tool-call arguments"
            );
        }
        other => panic!("expected WorkflowEvent::Progress(ToolUse), got {other:?}"),
    }
}
