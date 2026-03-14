# gRPC Remote Control

**Status:** WIP

## Summary

Add a `--grpc` flag to `tddy-coder` and `tddy-demo` that starts a gRPC server alongside the TUI. The gRPC server exposes a bidirectional streaming RPC that connects to the same Presenter instance as the TUI, enabling programmatic remote control of the application — sending `UserIntent`s and receiving `PresenterView` events — analogous to Selenium/Playwright for browser automation.

## Background

The TUI is the only way to interact with the running application. There is no programmatic interface for:

- Automated E2E testing of the full app (keyboard simulation + screen assertions)
- External tooling that wants to drive the workflow programmatically
- Integration with other systems that need to observe or control the TDD workflow

The Presenter follows MVP architecture: it accepts abstract `UserIntent`s (not raw key events) and fires `PresenterView` callbacks. This abstraction boundary is the natural point to attach a remote control interface.

## Requirements

### 1. New package: `tddy-grpc`

A workspace member that defines:

- Proto service definition (`.proto` file) with a single bidirectional streaming RPC
- Tonic-generated server and client code
- `GrpcService` implementation that bridges gRPC streams to the Presenter's event bus
- Accepts a Presenter event bus handle in its constructor (not the Presenter itself)

### 2. Proto service definition

```protobuf
service TddyRemote {
  rpc Connect(stream ClientMessage) returns (stream ServerMessage);
}
```

- **ClientMessage**: Carries `UserIntent` variants (SubmitFeatureInput, AnswerSelect, QueuePrompt, Quit, etc.)
- **ServerMessage**: Carries `PresenterView` callback events (mode_changed, activity_logged, goal_started, state_changed, workflow_complete, agent_output, inbox_changed)

Both directions carry the full set of events for bidirectional mirroring.

### 3. Event bus in Presenter

Replace the single `V: PresenterView` generic with a broadcast channel pattern:

- Presenter emits `PresenterEvent` variants to a broadcast channel
- TUI subscribes and maps events to its view updates
- gRPC subscribers receive the same events and stream them to connected clients
- UserIntents arrive from any subscriber (TUI keyboard, gRPC client) via an mpsc channel back to the Presenter

### 4. CLI integration

- `--grpc` flag added to both `CoderArgs` and `DemoArgs` (via shared `Args`)
- When `--grpc` is passed: start gRPC server on a configurable port (default: `50051`), then run TUI as normal
- Both TUI and gRPC operate on the same Presenter instance simultaneously
- Optional `--grpc-port <port>` to override the default

### 5. Daemon mode

The `--daemon` flag starts a headless gRPC server (no TUI) suitable for systemd deployment. The process runs indefinitely and serves multiple sessions sequentially.

**Session lifecycle via gRPC**:
- **Stream RPC**: Clients send `StartSession` with a prompt; daemon creates a session, runs the plan step, streams events (SessionCreated, ModeChanged with PlanReview for plan approval, WorkflowComplete). Client responds with `ApprovePlan`; the daemon automatically creates a worktree from `origin/master` and continues the workflow.
- **GetSession**: Returns session status (Active, Completed, Failed) by reading changeset.yaml from disk.
- **ListSessions**: Lists all sessions with their status.

**Session states**: Pending, Active, WaitingForInput, Completed, Failed.

**Git worktree management**:
- Each session gets a git worktree via `git worktree add` from `origin/master` (after `git fetch`). Worktrees live in `.worktrees/` relative to the repo root.
- The worktree path is persisted in `changeset.yaml` (`worktree`, `branch`, `repo_path`). The agent's working directory is set to the worktree path for post-plan steps.
- Branch and worktree names come from the plan agent's `branch_suggestion` and `worktree_suggestion`; worktree creation is automatic after plan approval (no separate elicitation).

**Agent commit & push**: The final workflow step's system prompt instructs the agent to commit all changes and push to the remote branch. The branch name is established during session creation and stored in the changeset.

**Changeset extensions**: `worktree`, `branch`, `remote_pushed` fields.

**Configuration**: Sessions base is `~/.tddy/sessions`. Port defaults to 50051 (`--grpc`). Graceful shutdown on SIGTERM. Optional `--web-port` and `--web-bundle-path` serve tddy-web static assets over HTTP alongside gRPC. When using LiveKit, `--livekit-api-key` and `--livekit-api-secret` (or env vars) generate tokens locally and auto-refresh by reconnecting before expiry; `--livekit-token` is an alternative for pre-generated tokens.

### 6. Terminal streaming (TUI mode)

When `--grpc` is set with TUI, the gRPC server exposes `StreamTerminal` RPC:

- **StreamTerminal**: Server-streaming RPC that delivers raw ANSI bytes from ratatui/crossterm rendering
- **TerminalOutput**: Message with `bytes data` — the exact byte stream a terminal would see
- **Use case**: Remote TUI viewer; clients pipe bytes into a terminal emulator to render the TUI remotely
- **Broadcast**: Multiple clients can subscribe; slow clients may miss frames (acceptable for terminal streaming)
- **Capture**: `CapturingWriter` in tddy-tui wraps stdout and invokes a callback on each write; tddy-coder wires a broadcast channel to `TddyRemoteService::with_terminal_bytes`

### 7. Codegen tooling

- Use **Buf** for proto management and code generation (prost plugin)
- Add `buf` to the Nix flake devShell
- Proto files live in `packages/tddy-grpc/proto/`

## Success Criteria

1. `tddy-demo --grpc` starts both TUI and gRPC server
2. A gRPC client can connect and send a `SubmitFeatureInput` intent
3. The gRPC client receives `PresenterView` events (mode changes, activity log entries, etc.) as the workflow progresses. When a workflow completes with an empty inbox, clients receive `ModeChanged(FeatureInput)` and can immediately send a new `SubmitFeatureInput` to start another workflow
4. The TUI and gRPC client see the same state — sending an intent from either side updates both
5. Tests in `tddy-grpc` verify bidirectional event flow using an in-process gRPC client connected to a test Presenter instance
6. `cargo test -p tddy-grpc` passes with all acceptance tests

### E2E Testing

The `tddy-e2e` package provides end-to-end tests using this gRPC interface:

- **gRPC-driven tests**: Connect as a client, send intents, assert on presenter events (e.g. `grpc_clarification`, `grpc_full_workflow`)
- **PTY tests**: Spawn the binary in a pseudo-terminal, assert on rendered screen output (`pty_clarification` with termwright, run with `--ignored`)
- Uses `tddy-demo` with StubBackend for deterministic, fast tests

## Affected Features

- [planning-step.md](planning-step.md) — Presenter architecture changes (event bus)
- [implementation-step.md](implementation-step.md) — Presenter architecture changes (event bus)
