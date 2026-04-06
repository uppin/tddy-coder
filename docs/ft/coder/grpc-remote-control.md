# gRPC Remote Control

**Status:** Stable

## Summary

`tddy-coder` and `tddy-demo` support a `--grpc` flag that starts a gRPC server alongside the TUI. The gRPC server exposes a bidirectional streaming RPC that connects to the same Presenter instance as the TUI, enabling programmatic remote control of the application — sending `UserIntent`s and receiving `PresenterView` events — analogous to Selenium/Playwright for browser automation.

## Background

The TUI is the only way to interact with the running application. There is no programmatic interface for:

- Automated E2E testing of the full app (keyboard simulation + screen assertions)
- External tooling that wants to drive the workflow programmatically
- Integration with other systems that need to observe or control the TDD workflow

The Presenter follows MVP architecture: it accepts abstract `UserIntent`s (not raw key events) and fires `PresenterView` callbacks. This abstraction boundary is the natural point to attach a remote control interface.

## Transport stack (reference)

Remote control and terminal streaming use these workspace crates:

- **`tddy-rpc`**: Transport-agnostic RPC types, streaming, `RpcService` trait, optional tonic adapters.
- **`tddy-codegen`**: Generates service traits and server glue from protos.
- **`tddy-service`**: Service implementations (`TddyRemoteService`, `DaemonService`, `TerminalService`, token and auth handlers). Tonic-generated code lives here; protos under `packages/tddy-service/proto/`.
- **`tddy-livekit`**: LiveKit participant adapter: maps `RpcRequest` ↔ `RpcMessage` over data channels; depends on `tddy-rpc` for the envelope protocol.
- **`tddy-connectrpc`**: Connect-RPC HTTP handlers for browser clients (e.g. `TokenService`, `AuthService`) where applicable.

Application code wires `tddy-service` + `tddy-livekit` (and Connect-RPC) at runtime.

## Requirements

### 1. Service layer: `tddy-service`

The workspace member that defines:

- Proto service definitions (`.proto` files) including bidirectional streaming `TddyRemote`
- Tonic-generated server and client code
- `TddyRemoteService` / `DaemonService` implementations that bridge gRPC streams to the Presenter's event bus
- Accepts a Presenter event bus handle in constructors (not the Presenter itself)

### 2. Proto service definition

```protobuf
service TddyRemote {
  rpc Connect(stream ClientMessage) returns (stream ServerMessage);
}
```

- **ClientMessage**: Carries `UserIntent` variants (SubmitFeatureInput, AnswerSelect, QueuePrompt, Quit, etc.)
- **ServerMessage**: Carries `PresenterView` callback events (mode_changed, activity_logged, goal_started, state_changed, workflow_complete, agent_output, inbox_changed) plus **`session_runtime_status`** ([`SessionRuntimeStatus`](../../../packages/tddy-service/proto/tddy/v1/remote.proto)) — full TUI-equivalent status line and structured fields for web / LiveKit clients

Both directions carry the full set of events for bidirectional mirroring.

### Live runtime status vs disk (Updated: 2026-03-22)

**Live** workflow and status-line updates for clients (especially the web terminal) **must** be observed via this **`TddyRemote`** stream from the running **`tddy-*` instance** — including **`session_runtime_status`** — with the UI **subscribing in real time**. The on-disk **changeset** (`changeset.yaml`) is **not** the authoritative channel for that live display; it remains persisted workflow state for resume and tooling. Do not replace stream subscription with changeset polling for active-session UI.

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
- **GetSession**: May return persisted session information by reading **changeset.yaml** from disk where applicable — useful for **catalog / resume**, not a substitute for **`TddyRemote`** stream events for **live** runtime status while a tool instance is connected.
- **ListSessions**: Lists sessions (often from session metadata on disk). Same distinction: **not** a replacement for subscribing to **`TddyRemote`** for **real-time** workflow status from the running **`tddy-*`** process.

**Session states**: Pending, Active, WaitingForInput, Completed, Failed.

**Git worktree management**:
- Each session gets a git worktree via `git worktree add` from `origin/master` (after `git fetch`). Worktrees live in `.worktrees/` relative to the repo root.
- The worktree path is persisted in `changeset.yaml` (`worktree`, `branch`, `repo_path`). The agent's working directory is set to the worktree path for post-plan steps.
- Branch and worktree names come from the plan agent's `branch_suggestion` and `worktree_suggestion`; worktree creation is automatic after plan approval (no separate elicitation).

**Agent commit & push**: The final workflow step's system prompt instructs the agent to commit all changes and push to the remote branch. The branch name is established during session creation and stored in the changeset.

**Changeset extensions**: `worktree`, `branch`, `remote_pushed` fields.

**Configuration**: Sessions base is `~/.tddy/sessions`. Port defaults to 50051 (`--grpc`). Graceful shutdown on SIGTERM. Optional `--web-port` and `--web-bundle-path` serve tddy-web static assets over HTTP alongside gRPC. When using LiveKit, `--livekit-api-key` and `--livekit-api-secret` (or env vars) generate tokens locally and auto-refresh by reconnecting before expiry; `--livekit-token` is an alternative for pre-generated tokens.

**Session metadata (daemon spawn)**: When spawned by `tddy-daemon`, `tddy-coder` writes `.session.yaml` with `project_id` (and accepts `--project-id`). Worktrees under the repo still follow `tddy-core` worktree flow from the **main repo** path; see [daemon project concept](../../daemon/project-concept.md).

### 6. Terminal streaming (TUI mode and daemon)

**StreamTerminal** (TUI mode with `--grpc`): Server-streaming RPC delivers raw ANSI bytes from ratatui/crossterm. Multiple clients share one broadcast; slow clients may miss frames.

**StreamTerminalIO** (daemon with LiveKit or gRPC): Per-connection virtual TUI. Each RPC connection gets its own headless ratatui instance (`VirtualTui`) via `ViewConnection`. Presenter exposes `connect_view()` → state snapshot + event_rx + intent_tx. Client keyboard input is parsed by the virtual TUI into `UserIntent`s. When the client disconnects, the virtual TUI is stopped. `TerminalServiceImplPerConnection` and `TddyRemoteService::with_view_connection_factory` create one VirtualTui per `StreamTerminalIO` call. Daemon with LiveKit exposes TerminalService (per-connection VirtualTui) instead of EchoService.

**Terminal resize**: Local event loop handles `Event::Resize` with `terminal.clear()` for a clean redraw. Virtual TUI accepts `\x1b]resize;cols;rows\x07`; after `terminal.resize()` it calls `terminal.clear()` and resets the frame buffer so the next render sends a full frame to the client. Scroll offsets are clamped after resize.

### 7. Codegen tooling

- Use **Buf** for proto management and code generation (prost plugin) where configured
- Add `buf` to the Nix flake devShell
- Proto files live in `packages/tddy-service/proto/` (and LiveKit envelope protos under `packages/tddy-livekit/proto/` as needed)

## Success Criteria

1. `tddy-demo --grpc` starts both TUI and gRPC server
2. A gRPC client can connect and send a `SubmitFeatureInput` intent
3. The gRPC client receives `PresenterView` events (mode changes, activity log entries, etc.) as the workflow progresses. When a workflow completes with an empty inbox, clients receive `ModeChanged(FeatureInput)` and can immediately send a new `SubmitFeatureInput` to start another workflow
4. The TUI and gRPC client see the same state — sending an intent from either side updates both
5. Integration tests in `tddy-service` and end-to-end tests in `tddy-e2e` verify bidirectional event flow against a running gRPC server and presenter
6. `cargo test -p tddy-service` and `cargo test -p tddy-e2e` cover the remote-control surface

### E2E Testing

The `tddy-e2e` package provides end-to-end tests using this gRPC interface:

- **gRPC-driven tests**: Connect as a client, send intents, assert on presenter events (e.g. `grpc_clarification`, `grpc_full_workflow`)
- **PTY tests**: Spawn the binary in a pseudo-terminal, assert on rendered screen output (`pty_clarification` with termwright, run with `--ignored`)
- Uses `tddy-demo` with StubBackend for deterministic, fast tests

## Affected Features

- [planning-step.md](planning-step.md) — Presenter architecture changes (event bus)
- [implementation-step.md](implementation-step.md) — Presenter architecture changes (event bus)
