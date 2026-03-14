# Changeset: Workflow Restart on Completion

## Planning Context (from Plan Mode)

See [PRD-2026-03-14-workflow-restart-on-completion.md](../../ft/coder/1-WIP/PRD-2026-03-14-workflow-restart-on-completion.md).

**Summary:** After successful workflow completion, transition to FeatureInput mode instead of Done, allowing users to immediately start a new workflow without restarting. Applies to both local TUI and daemon/gRPC mode.

**Design decisions:**
- SubmitFeatureInput detects dead channel via `send()` failure, calls `restart_workflow()`
- `is_done()` checks `workflow_result.is_some()` instead of `AppMode::Done`
- `AppMode::Done` variant kept for backward compatibility

## Affected Packages

- `tddy-core` — Presenter WorkflowComplete handler, SubmitFeatureInput restart, is_done(), restart_workflow helper
- `tddy-tui` — VirtualTui apply_event WorkflowComplete
- `tddy-e2e` — pty_full_workflow screen text detection
- `tddy-coder` — presenter_integration restart test

## Implementation Milestones

- [x] WorkflowComplete handler: Done → FeatureInput
- [x] SubmitFeatureInput: detect dead channel, restart_workflow
- [x] restart_workflow helper
- [x] is_done() uses workflow_result
- [x] Inbox dequeue clears workflow_result
- [x] VirtualTui WorkflowComplete: FeatureInput
- [x] Unit test: success → FeatureInput
- [x] E2E: screen text detection
- [x] Integration test: restart after completion

## Acceptance Tests

1. Success completion transitions to FeatureInput (not Done)
2. SubmitFeatureInput after completion spawns new workflow
3. gRPC clients see FeatureInput after completion
