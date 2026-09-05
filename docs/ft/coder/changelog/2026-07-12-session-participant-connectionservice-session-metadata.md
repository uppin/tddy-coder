# 2026-07-12 — Session-participant ConnectionService + session metadata + shared tool engine

- A `tddy-coder` session now serves session-scoped `ConnectionService` methods (`ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`, `WatchTerminalControl`) directly from its own LiveKit participant (`daemon-{instanceId}-{sessionId}`), and publishes a `session` metadata block (goal/state/agent/model/…) on workflow-state transitions, shallow-merged with `owned_project_count` / `codex_oauth`.
- `ExecuteTool` dispatches through a new shared **`tddy-tool-engine`** crate (extracted from the daemon) against the session's worktree root, backed by a per-session `tddy_task::TaskRegistry`; the catalog mirrors the shared engine's. The `ToolExecutor` seam is `async`.
- `DeleteSession` / `SignalSession` are **not** served by the coder — the web routes them daemon-direct to `daemon-{instanceId}`.
- The interactive path wires a workflow-state tap (`spawn_session_metadata_tap`) so transitions republish the `session` block; the headless `--grpc` path's metadata tap is FIXME-tracked.
- Feature: [session-participant-rpc.md](../session-participant-rpc.md). PR [#297](https://github.com/uppin/tddy-coder/pull/297).
