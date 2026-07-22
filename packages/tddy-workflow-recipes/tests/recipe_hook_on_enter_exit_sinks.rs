//! Acceptance test: recipe `RunnerHooks` impls wire `AgentOutputSink` via
//! `on_enter_task` / `on_exit_task` (the new generic lifecycle hooks).
//!
//! Feature: docs/ft/coder/discovery-agent.md (Phase A criterion 6)
//! Changeset: docs/dev/1-WIP/2026-06-24-changeset-tddy-graph-extraction.md
//!
//! After the `tddy-graph` extraction, `RunnerHooks::agent_output_sink()` and
//! `RunnerHooks::progress_sink()` are removed from the trait. The concrete impls in
//! `tddy-workflow-recipes` must instead override `on_enter_task` to call
//! `tddy_core::workflow::set_sinks(...)` and `on_exit_task` to call
//! `tddy_core::workflow::clear_sinks()`. This test verifies that the lifecycle hooks correctly
//! wire and clear the thread-local agent-output sink.

use std::sync::mpsc;
use std::time::Duration;

use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
// get_agent_sink / get_progress_sink are re-exported from tddy_core::workflow
use tddy_core::workflow::{get_agent_sink, get_progress_sink};
use tddy_core::ProgressEvent;
use tddy_workflow_recipes::free_prompting::FreePromptingWorkflowHooks;

/// `FreePromptingWorkflowHooks::on_enter_task` must call `set_sinks(...)`, making the
/// thread-local `AgentOutputSink` available to tasks running in the same thread.
/// `on_exit_task` must call `clear_sinks()`, leaving the sink absent after the task returns.
#[test]
fn free_prompting_hooks_on_enter_sets_sink_and_on_exit_clears_it() {
    // Given — a hooks instance with an event channel (sink source)
    let (tx, rx) = mpsc::channel::<WorkflowEvent>();
    let hooks = FreePromptingWorkflowHooks::new(Some(tx));
    let ctx = Context::new();
    let task_id = "prompting";

    // Pre-condition: no sink is set before on_enter_task
    assert!(
        get_agent_sink().is_none(),
        "no agent sink should be present before on_enter_task"
    );

    // When
    hooks.on_enter_task(task_id, &ctx);

    // Then — the thread-local sink is populated
    let sink = get_agent_sink().expect(
        "on_enter_task must call set_sinks so that the agent output sink \
         is available to task.run() in the same thread",
    );

    // And — emitting through the thread-local sink delivers events to the channel
    sink.emit("streamed output line");
    let ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("sink emit must deliver WorkflowEvent::AgentOutput to the presenter channel");
    assert!(
        matches!(ev, WorkflowEvent::AgentOutput(ref s) if s == "streamed output line"),
        "expected AgentOutput(\"streamed output line\"), got {:?}",
        ev
    );

    // When
    hooks.on_exit_task(task_id, &ctx);

    // Then — the thread-local sink is cleared
    assert!(
        get_agent_sink().is_none(),
        "on_exit_task must call clear_sinks so the sink is not visible to subsequent tasks"
    );
}

/// `FreePromptingWorkflowHooks::on_enter_task` must also call `set_sinks(...)` with a populated
/// `ProgressSink`, making the thread-local progress sink available to tasks running in the same
/// thread — this is what lets `FastContextBackend` (which has no native CLI stream to parse)
/// surface `ProgressEvent::ToolUse` in the activity pane. `on_exit_task` must clear it.
#[test]
fn free_prompting_hooks_on_enter_sets_progress_sink_and_on_exit_clears_it() {
    // Given — a hooks instance with an event channel (sink source)
    let (tx, rx) = mpsc::channel::<WorkflowEvent>();
    let hooks = FreePromptingWorkflowHooks::new(Some(tx));
    let ctx = Context::new();
    let task_id = "prompting";

    // Pre-condition: no progress sink is present before on_enter_task
    assert!(
        get_progress_sink().is_none(),
        "no progress sink should be present before on_enter_task"
    );

    // When
    hooks.on_enter_task(task_id, &ctx);

    // Then — the thread-local sink is populated
    let sink = get_progress_sink().expect(
        "on_enter_task must call set_sinks so that the progress sink \
         is available to task.run() in the same thread",
    );

    // And — emitting through the thread-local sink delivers events to the channel
    sink.emit(&ProgressEvent::ToolUse {
        name: "GLOB".to_string(),
        detail: None,
        input_json: None,
        call_id: None,
    });
    let ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("sink emit must deliver WorkflowEvent::Progress to the presenter channel");
    assert!(
        matches!(
            ev,
            WorkflowEvent::Progress(ProgressEvent::ToolUse { ref name, .. }) if name == "GLOB"
        ),
        "expected Progress(ToolUse {{ name: \"GLOB\", .. }}), got {:?}",
        ev
    );

    // When
    hooks.on_exit_task(task_id, &ctx);

    // Then — the thread-local sink is cleared
    assert!(
        get_progress_sink().is_none(),
        "on_exit_task must call clear_sinks so the progress sink is not visible to subsequent tasks"
    );
}

/// `on_enter_task` and `on_exit_task` are no-ops by default on the trait (they have default
/// `{}` impls). A hooks struct that does NOT override them must not panic.
#[test]
fn hooks_impl_without_sink_does_not_panic_on_enter_or_exit() {
    use std::error::Error;
    use tddy_core::workflow::task::TaskResult;

    struct MinimalHooks;
    impl RunnerHooks for MinimalHooks {
        fn before_task(&self, _: &str, _: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
        fn after_task(
            &self,
            _: &str,
            _: &Context,
            _: &TaskResult,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
        fn on_error(&self, _: &str, _: &Context, _: &(dyn Error + Send + Sync)) {}
    }

    // Given — no sink is set initially
    let hooks = MinimalHooks;
    let ctx = Context::new();

    // When / Then — calling the default no-op lifecycle hooks must not panic
    hooks.on_enter_task("some_task", &ctx);
    assert!(
        get_agent_sink().is_none(),
        "default on_enter_task must not set any sink"
    );
    hooks.on_exit_task("some_task", &ctx);
    assert!(
        get_agent_sink().is_none(),
        "default on_exit_task must not leave any sink set"
    );
}
