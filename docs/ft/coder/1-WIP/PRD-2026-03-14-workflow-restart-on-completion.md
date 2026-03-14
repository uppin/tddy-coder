# PRD: Workflow Restart on Completion

## Summary

When a TDD workflow completes successfully, the terminal currently shows "Workflow complete. Press Enter to exit." and the user must restart the application to begin a new workflow. Instead, the terminal should transition back to the FeatureInput (Init) state, allowing the user to immediately start a new workflow without restarting.

## Background

The application uses a Presenter/AppMode state machine. On workflow completion, the mode transitions from `Running` → `Done`. In Done mode, the only available actions are Enter (quit) or Q (quit). This forces users to restart the entire application for each workflow, which is friction-heavy — especially in daemon/gRPC mode where restarting means reconnecting all clients.

## Affected Features

- [planning-step.md](../planning-step.md) — workflow lifecycle
- [implementation-step.md](../implementation-step.md) — workflow lifecycle
- [grpc-remote-control.md](../grpc-remote-control.md) — gRPC clients see mode transitions

## Proposed Changes

### What changes
- On successful `WorkflowComplete`, transition to `AppMode::FeatureInput` instead of `AppMode::Done`
- Preserve the activity log from the completed workflow (user can scroll back)
- Apply to both local TUI and daemon/gRPC mode

### What stays the same
- Error case: `ErrorRecovery` mode behavior is unchanged
- Inbox dequeue behavior: if items are queued, they still auto-start (existing logic)
- Exit mechanism: Ctrl+C remains the way to exit from FeatureInput
- `AppMode::Done` variant may remain in the enum for backward compatibility or be removed if not needed elsewhere

## Requirements

1. When workflow completes successfully (and inbox is empty), mode transitions to `FeatureInput` instead of `Done`
2. Activity log is preserved — user sees completion summary and can scroll back to previous output
3. FeatureInput prompt is shown, allowing the user to type a new feature description
4. The Presenter must be ready to accept a new `SubmitFeatureInput` intent and start a new workflow
5. gRPC/daemon clients must receive the `ModeChanged(FeatureInput)` event after completion
6. Exit from FeatureInput remains Ctrl+C only (current behavior)

## Success Criteria

- User completes a workflow → sees FeatureInput prompt → types new feature → new workflow starts
- Activity log from previous run is visible via scroll
- gRPC clients observe `FeatureInput` mode after workflow completion
- ErrorRecovery behavior is unchanged for failed workflows
- Existing tests updated to reflect new completion behavior
