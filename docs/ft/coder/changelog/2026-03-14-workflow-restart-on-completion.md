# 2026-03-14 — Workflow Restart on Completion

- **Completion behavior**: When a workflow completes successfully with an empty inbox, mode transitions to FeatureInput instead of Done. Users can immediately type a new feature and start another workflow without restarting.
- **Activity log**: Preserved after completion; user can scroll back to previous output.
- **gRPC/daemon**: Clients receive `ModeChanged(FeatureInput)` after WorkflowComplete and can send `SubmitFeatureInput` to start a new workflow.
- **Exit**: Ctrl+C remains the only way to exit from FeatureInput.
- **Packages**: tddy-core (WorkflowComplete handler, SubmitFeatureInput restart, is_done, restart_workflow), tddy-tui (VirtualTui FeatureInput on completion), tddy-e2e (pty/grpc completion assertions), tddy-coder (presenter_integration restart test).
