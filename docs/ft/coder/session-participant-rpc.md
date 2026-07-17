# Session Participant RPC & Metadata

## Overview

A `tddy-coder` session serves the session-scoped `ConnectionService` methods directly from its own LiveKit room participant (`daemon-{instanceId}-{sessionId}`) and publishes its session metadata on that participant. Once a web client has attached to a session, session-scoped RPCs — terminal I/O, terminal control, tools, VNC, screen-sharing — target the session participant. `DeleteSession` / `SignalSession` are **not** served by the coder: the web routes them directly to the daemon participant (`daemon-{instanceId}`), which owns process teardown and must stay reachable even when the coder participant is stuck. The daemon participant also remains the authority for attachment bootstrap (`StartSession`, `ConnectSession`, `ResumeSession`) and directory RPCs.

## Technical Context

### Design Rationale

Serving session-scoped RPCs from the coder's own LiveKit participant (rather than relaying
them through the daemon) binds the session screen to the session, not to the daemon, and
removes a hop. The coder publishes its own `session` metadata on that participant so the
web's sessions list can render active cross-host rows from presence alone — no `ListSessions`
fan-out to each owning daemon. `DeleteSession` / `SignalSession` stay **daemon-direct**
because the daemon owns process teardown and must remain reachable even when the coder
participant is stuck.

### Target Consumers

- **tddy-web `SessionsDrawerScreen`** — builds a `ConnectionService` client bound to the session participant for session-scoped RPCs; reads `session` metadata from the participant for the sessions list.
- **tddy-daemon** — remains the bootstrap/directory authority and the direct target for `DeleteSession` / `SignalSession` (called by the web, not relayed by the coder).

### Success Metrics

- **Latency**: session-scoped RPCs resolve on the session participant directly, without a daemon round-trip.
- **Presence richness**: active sessions show goal/state/agent/model in the web drawer without a `ListSessions` fan-out to the owning daemon.
- **Reliability**: delete/signal are daemon-direct, so they still work when the coder participant is stuck; daemon errors surface verbatim to the web caller.

## API/Library Requirements

### Core Capabilities

The coder's LiveKit participant serves the following `ConnectionService` methods (in addition to the existing `TerminalService`):

- `ListExecTools` — the session's executable tool catalog.
- `ListSessionToolCalls` — the durable `~/.tddy/sessions/{sessionId}/tool-calls.jsonl` log.
- `ExecuteTool` — invoke a tool against the session; appends to the same durable log. Dispatches through the shared **`tddy-tool-engine`** crate against the session's worktree root (`agent_working_dir`), backed by a per-session `tddy_task::TaskRegistry`; the catalog mirrors the shared engine's (`Read`/`Write`/`StrReplace`/`Delete`/`Grep`/`Glob`/`Shell`/`Await`/`ReadLints`/`SemanticSearch`). Background `Shell` jobs (`block_until_ms=0`) land in the registry; live status is reachable via `Await`. The `ToolExecutor` seam is `async`.
- `ClaimTerminalControl` / `WatchTerminalControl` — the coder owns its own terminal, so it serves the control lease directly (the daemon's `ClaudeCliSessionManager` control registry is irrelevant for tddy-coder sessions).
- `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` and `terminal_id`-addressed `StreamTerminalOutput` / `SendTerminalInput` — **bash terminal tabs.** The coder spawns interactive shell PTYs (`$SHELL`, fallback `/bin/bash`) in the session worktree via the shared **`tddy-pty`** crate and answers these RPCs addressed by `terminal_id`. The reserved `"main"` terminal remains the VirtualTui on `TerminalService` (it is not a PTY); `StopTerminalSession("main")` is rejected with `INVALID_ARGUMENT`. See [Session Terminal Tabs](../web/session-terminal-tabs.md). Because the coder already runs as the target OS user, its PTYs spawn without privilege-drop (`os_user: None`).
- `DeleteSession` / `SignalSession` — **not served here**. The web routes them directly to the daemon participant (`daemon-{instanceId}`), which owns process teardown. This keeps lifecycle control available even when the coder participant is stuck, and avoids a relay hop.

### Developer Experience (DX) Requirements

- The session-scoped surface is reachable on the same LiveKit identity the web already uses for terminal I/O (`daemon-{instanceId}-{sessionId}`); no new identity or room.
- Delete/signal stay on the existing daemon participant client the web already holds (`daemon-{instanceId}`); no new path.
- Metadata publishes on the existing participant metadata channel; consumers parse it with a stable JSON schema.

## Technical Requirements

### API Contract

- **Transport**: LiveKit data-channel `tddy-rpc` (same as `TerminalService` today). The coder registers a `ConnectionService` `ServiceEntry` alongside `TerminalService` on its participant.
- **Auth**: every method validates `session_token` exactly as the daemon does today (GitHub user → OS user → session ownership). Delete/signal are validated at the daemon (the web passes the caller's `session_token`).
- **Method scope**: session-scoped methods only. `StartSession`, `ConnectSession`, `ResumeSession`, `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`, `ListProjectBranches`, `DeleteSession`, `SignalSession` are **not** served by the coder participant.

### Participant Metadata

The coder publishes a JSON document on its local participant metadata, shallow-merged with existing `owned_project_count` / `codex_oauth` keys via `merge_participant_metadata_json`:

```json
{
  "session": {
    "workflow_goal": "acceptance-tests",
    "workflow_state": "Red",
    "elapsed_display": "3m",
    "agent": "claude",
    "model": "sonnet-4",
    "activity_status": "",
    "recipe": "tdd",
    "repo_path": "/home/user/repos/foo",
    "pending_elicitation": false
  }
}
```

Republished on workflow state transitions (the workflow already writes `changeset.yaml` on each transition). Field shapes mirror the daemon's `SessionListStatusDisplay` enrichment so the web can render them identically.

### Architecture

- Modules in `tddy-coder`: `connection_service_participant` (method handlers — tools, terminal control), `metadata_publisher` (changeset → `session` JSON → `metadata_tx`), and `spawn_session_metadata_tap` (workflow-state → `session` metadata on the interactive path). No relay module: delete/signal are daemon-direct.
- `ExecuteTool` dispatches via the shared [`tddy-tool-engine`](../../../packages/tddy-tool-engine/) crate (`CoderSessionToolExecutor` holds the session's `worktree_root` + a `tddy_task::TaskRegistry`); `coder_session_tool_catalog()` mirrors the shared catalog.
- Wired into the participant's `ServiceEntry` list in `run.rs` (both the `livekit_multi` path and the single-token path) and into the existing `metadata_tx` watch channel. The headless `--grpc` path's `session` metadata tap is FIXME-tracked (`2026-07-12-fast-session-change`); the tool executor is wired on both paths.

### Performance Constraints

- Metadata republish only on state transitions (not per keystroke) — bounded write rate.
- Delete/signal stay daemon-direct (no extra hop); the daemon owns process lifecycle.

### Integration Patterns

- The web constructs a `ConnectionService` client via `liveKitFactory(room, sessionServerIdentity)` and calls session-scoped methods on it; bootstrap/directory RPCs **and `DeleteSession` / `SignalSession`** continue to use the daemon participant client.
- The web's `useRoomParticipants` parses the `session` metadata block; `SessionManager` overlays it onto sessions-list rows.

## Integration Examples

### Web calling a session-scoped RPC through the participant

```
client = createClient(ConnectionService, liveKitFactory(room, "daemon-west-1-<sessionId>"))
await client.executeTool({ sessionToken, sessionId, toolName, argsJson })
```

### Web reading session metadata from a participant

```
participant.metadata → JSON.parse → { session: { workflow_goal, workflow_state, agent, model, ... } }
```

## Acceptance Criteria

- [x] The coder's LiveKit participant answers `ListExecTools`, `ExecuteTool`, and `ClaimTerminalControl` over its identity.
- [x] After a workflow state transition, the participant's metadata JSON contains a `session` block with `workflow_goal`, `workflow_state`, `agent`, and `model`.
- [x] `DeleteSession` / `SignalSession` are **not** served by the coder participant; the web routes them to `daemon-{instanceId}` and they still terminate the session (daemon-direct).
- [x] The `session` metadata key coexists with `owned_project_count` / `codex_oauth` (shallow merge preserves sibling keys).
- [x] Start/Resume/Connect and directory RPCs are **not** served by the coder participant.

## Testing Strategy

- **Integration (LiveKit testkit)**: spawn a coder participant against `LIVEKIT_TESTKIT_WS_URL`; call session-scoped `ConnectionService` methods (tools, terminal control) over its identity; observe metadata after a transition. A real-executor test asserts `ListExecTools` returns the full shared catalog and `ExecuteTool("Read")` returns the worktree file's contents via `tddy-tool-engine`. Delete/signal daemon-direct behaviour is covered by the web Cypress + daemon unit tests.
- **Unit**: `connection_service_participant` handlers against an in-process harness (incl. an async `ToolExecutor` fake, catalog-mirrors-shared-engine, `CoderSessionToolExecutor` Write→Read against a tempdir); `metadata_publisher` JSON shape + merge + the workflow-state tap mapping (`apply_session_metadata_event`).

## Related Documentation

- [Session Drawer Screen § Fast Session Change](../web/session-drawer.md#fast-session-change) — web consumer.
- [Terminal Sessions § Session-scoped RPC routing & daemon-direct lifecycle](../daemon/terminal-sessions.md#session-scoped-rpc-routing--daemon-direct-lifecycle) — daemon side.
- [LiveKit peer discovery (daemon)](../daemon/livekit-peer-discovery.md) — daemon participant identity.
- [LiveKit common room: owned project count](../web/livekit-participant-owned-projects.md) — participant metadata schema.
- [`participant-metadata.md`](../../../packages/tddy-livekit/docs/participant-metadata.md) — `tddy-livekit` metadata merge technical reference.
- [`tddy-tool-engine`](../../../packages/tddy-tool-engine/) — shared exec-tool dispatch engine.
