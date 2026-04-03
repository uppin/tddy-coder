//! The TUI activity pane shows assistant text from [`WorkflowEvent::AgentOutput`]. [`RunnerHooks::agent_output_sink`]
//! must forward streaming lines from backends (e.g. Cursor) the same way [`tddy_workflow_recipes::tdd::TddWorkflowHooks`] does.

use std::sync::mpsc;
use std::time::Duration;

use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_workflow_recipes::free_prompting::FreePromptingWorkflowHooks;

#[test]
fn free_prompting_hooks_wire_agent_output_sink_to_workflow_events() {
    let (tx, rx) = mpsc::channel::<WorkflowEvent>();
    let hooks = FreePromptingWorkflowHooks::new(Some(tx));
    let sink = hooks
        .agent_output_sink()
        .expect("free-prompting hooks must expose AgentOutputSink so assistant streaming reaches the activity pane");
    sink.emit("streamed assistant fragment");

    let ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("sink emit must deliver WorkflowEvent::AgentOutput to the presenter channel");

    assert!(
        matches!(ev, WorkflowEvent::AgentOutput(ref s) if s == "streamed assistant fragment"),
        "expected AgentOutput with streamed text, got {:?}",
        ev
    );
}
