# 2026-03-09 — MVP Architecture Refactoring

- **Presenter** (tddy-core): Owns application state and workflow orchestration. Receives abstract `UserIntent`s (no KeyEvents). Spawns workflow thread; polls `WorkflowEvent`; forwards to `PresenterView` callbacks.
- **tddy-tui** (new package): Ratatui View layer. Implements `PresenterView`; maps crossterm keys to `UserIntent`; holds view-local state (scroll, text buffers, selection cursor); renders activity log, status bar, prompt bar, inbox.
- **tddy-coder**: Removed `tui/` module. Uses Presenter + TuiView + `run_event_loop`. Re-exports presenter types from tddy-core; `disable_raw_mode` from tddy-tui.
- **Integration tests**: Scenario-based `presenter_integration.rs` with TestView + StubBackend. Covers full workflow, clarification round-trip, inbox queue/dequeue, workflow error handling.
- **Done mode**: TUI stays open after workflow completes; user presses Enter or Q to exit. Workflow result printed on exit.
- **User impact**: No change to CLI behavior, TUI layout, or workflow steps.
