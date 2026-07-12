# Fast Session Change — PRD

**Date**: 2026-07-12
**PRD Type**: Enhancement

## Affected Features

**CRITICAL**: List ALL feature documents affected by this PRD:

- **Primary Feature**: [Session Drawer Screen](../session-drawer.md) — per-session runtime registry + background terminals; session-participant RPC routing; inspector I/O bytes + last-data-received; sessions list reads session metadata from LiveKit participants.
- **Related Feature 1**: [Web Terminal](../web-terminal.md) — background terminal lifecycle and per-session LiveKit room ownership; participant-metadata-driven sessions list.
- **Related Feature 2**: [LiveKit common room: owned project count](../livekit-participant-owned-projects.md) — extend participant metadata JSON schema with a `session` block (sibling of `owned_project_count` / `codex_oauth`), shallow-merged by `merge_participant_metadata_json`.

## Summary

The sessions drawer screen today holds a single attached session at a time: selecting another session unmounts the Ghostty terminal, tears down its LiveKit room, and re-connects/re-resizes on the new selection. All session-scoped `ConnectionService` RPCs route through the daemon's `daemon-{instanceId}` participant. This PRD makes each attached session own a self-contained runtime — its own LiveKit Room, its own Ghostty terminal, its own RPC client bound to the session's room participant (`daemon-{instanceId}-{sessionId}`) — kept alive in the background while the user navigates between sessions, with the inspector showing live I/O byte counts and a "last data received: Ns ago" relative timestamp. It also teaches the sessions list to read session metadata published by each coder process on its LiveKit participant, so active cross-host rows show goal/state/agent/model without a `ListSessions` fan-out.

## Background

Fast context switching between parallel sessions is the core workflow for operators running several tddy-coder sessions at once. Today every switch pays a full LiveKit reconnect + terminal reinit + resize round-trip, the switched-away session stops receiving bytes (so its screen is stale on return), and session metadata for cross-host active sessions is unavailable without polling every daemon. Binding the screen to the session's own participant and keeping per-session runtimes alive removes the reconnect tax, lets background sessions keep streaming, and makes presence itself carry the metadata.

## Proposed Changes

### What's Changing

#### [Session Drawer Screen](../session-drawer.md)

- **Per-session runtime registry.** Replace the single `useSessionAttachment` singleton with a `SessionRuntimeRegistry` keyed by `sessionId`. Each `SessionRuntimeState` holds: attachment status, LiveKit `Room`, `ConnectionService` client bound to the session participant (`daemon-{instanceId}-{sessionId}`), `GhosttyTerminalHandle`, traffic meter, byte counters (in/out), `lastDataReceivedAt`, terminal control state.
- **Background terminals (req 2, 3).** One `<SessionRuntime>` is mounted per attached session. The focused session's terminal is CSS-visible; the others are `display:none` but stay subscribed to `streamTerminalIO`. Selecting a session is a focus switch — no unmount, no `resetAttachment`, no LiveKit reconnect, no terminal resize.
- **Eviction.** Background attachments stay alive until explicit disconnect (no cap). Disconnect removes only that session's runtime.
- **Session-participant RPC routing (req 1).** For an attached LiveKit session, build the `ConnectionService` client via `liveKitFactory(room, sessionServerIdentity)` and route `ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`, `WatchTerminalControl`, VNC, and screen-sharing RPCs through it. `DeleteSession` / `SignalSession` are **daemon-direct**: the web routes them to `daemon-{instanceId}` (not the session participant), so lifecycle control still works when the coder participant is stuck. `ConnectSession` / `ResumeSession` / `StartSession` (attachment bootstrap) and directory RPCs (`ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`, `ListProjectBranches`) also stay on the daemon participant.
- **Sessions list metadata from participants (req 4).** `useRoomParticipants`'s `RoomParticipant` gains a parsed `session` metadata field. `SessionManager.mergeActiveAndFetchedSessions` overlays parsed session metadata onto synthesized cross-host rows and live-updates fetched rows from participant metadata (presence-driven, no `ListSessions` fan-out for active rows).
- **Inspector I/O bytes + last-data-received (req 5).** The inspector Details tab shows bytes in, bytes out, and a "last data received: Ns ago" relative timestamp that advances while the inspector is open. **Dual source**: for an attached LiveKit session, bytes in/out come from the per-session traffic meter and `lastDataReceivedAt` from the LiveKit transport's `DataReceived` events (updating in the background). For a session with **no LiveKit participant** (stopped tddy-coder session, or claude-cli / cursor-cli / workspace sessions that never join a room), the inspector falls back to `SessionEntry` fields (`bytes_in`, `bytes_out`, `last_data_received_at`) populated by the daemon `ListSessions` RPC. The inspector renders the live runtime values when a runtime exists, else the daemon-sourced `SessionEntry` values.

#### [Web Terminal](../web-terminal.md)

- Documents that each attached LiveKit session owns its own `Room` (joined as `browser-{sessionId}-{ts}`) and its own `GhosttyTerminalLiveKit` instance, kept mounted in the background per the session drawer's runtime registry.
- Notes that the per-session LiveKit room is **not** the shared `livekit.common_room` presence connection; it is the session's terminal room (same room name the coder participant joined, distinct browser identity).

#### [LiveKit common room: owned project count](../livekit-participant-owned-projects.md)

- Extends the participant metadata JSON schema with a `session` object (sibling of `owned_project_count` / `codex_oauth`) carrying: `workflow_goal`, `workflow_state`, `elapsed_display`, `agent`, `model`, `activity_status`, `recipe`, `repo_path`, `pending_elicitation`. Published by the coder process (see the new coder feature doc). Shallow-merged at the top level so all keys coexist.
- `useRoomParticipants` parses `session` metadata; the sessions drawer overlays it onto synthesized and fetched rows.

### What's Staying the Same

- The shared `livekit.common_room` presence connection and the **Connected participants** table are unchanged.
- `ConnectionScreen` (`#/`) is unchanged; this PRD affects only `SessionsDrawerScreen` (`#/sessions`).
- claude-cli / cursor-cli / workspace sessions keep their current gRPC terminal path (`GrpcSessionTerminal`); they have no LiveKit participant and are out of scope for the participant-binding and background-terminal requirements.
- `ListSessions` disk enrichment (`.session.yaml` / `changeset.yaml`) remains the source of truth for inactive rows and for fields not yet published on the participant.

## Impact Analysis

### Technical Impact

- **tddy-web**: new `SessionRuntimeRegistry` store + React binding; `SessionsDrawerScreen` and `SessionMainPane` restructured to mount one runtime per attached session; new `sessionParticipantMetadata` parser; `SessionManager` merge extended; inspector extended with byte counters + last-received relative time; RPC client construction split between session-participant (session-scoped RPCs) and daemon-participant (bootstrap/directory).
- **tddy-coder**: serves session-scoped `ConnectionService` from its LiveKit participant; publishes `session` metadata. See the new coder feature doc.
- **tddy-livekit**: documents the `session` metadata key; the merge helper already exists.
- **tddy-daemon**: remains the bootstrap/directory authority and the direct target for `DeleteSession` / `SignalSession` (called by the web, not relayed by the coder). See the daemon PRD.
- **Performance**: memory grows with the number of concurrently attached sessions (one LiveKit Room + one Ghostty terminal each). Eviction is explicit-disconnect only (confirmed decision). No cap is introduced in this PRD.

### User Impact

- Switching between attached sessions is instant (no reconnect/resize); background sessions keep streaming and are not stale on return.
- The inspector shows live traffic and a "last data received" timestamp per session, even for background sessions.
- Cross-host active sessions show goal/state/agent/model in the drawer without waiting for `ListSessions`.

## Implementation Plan

1. Introduce `SessionRuntimeRegistry` + `SessionRuntime` (web) with per-session Room, terminal, traffic meter, byte counters, last-received timestamp, and a session-participant `ConnectionService` client.
2. Restructure `SessionsDrawerScreen` / `SessionMainPane` to mount one runtime per attached session; focus switch = CSS visibility, no unmount.
3. Route session-scoped RPCs through the session participant; keep bootstrap/directory RPCs on the daemon participant.
4. Parse `session` participant metadata; overlay it in `SessionManager.mergeActiveAndFetchedSessions`.
5. Extend the inspector with bytes in/out + last-data-received relative time.
6. Coordinate with the coder (serve `ConnectionService`, publish metadata) and daemon (delete/signal daemon-direct) PRDs.

## Acceptance Criteria

- [ ] Switching focus between two attached LiveKit sessions does not unmount or resize either terminal; the switched-away terminal keeps receiving bytes. ([Session Drawer Screen](../session-drawer.md))
- [ ] Session-scoped RPCs (`ExecuteTool`, `ClaimTerminalControl`, VNC, screen-sharing) are dispatched to the session participant identity, not `daemon-{instanceId}`. ([Session Drawer Screen](../session-drawer.md))
- [ ] `DeleteSession` / `SignalSession` are dispatched to `daemon-{instanceId}` (daemon-direct), not the session participant, even for an attached session. ([Session Drawer Screen](../session-drawer.md))
- [ ] `StartSession` / `ConnectSession` / `ResumeSession` continue to target `daemon-{instanceId}`. ([Session Drawer Screen](../session-drawer.md))
- [ ] The inspector shows bytes in > 0, bytes out, and a "last data received: Ns ago" relative string that advances over time. ([Session Drawer Screen](../session-drawer.md))
- [ ] For a session with no LiveKit participant, the inspector renders bytes in/out + last-data-received from the daemon `ListSessions` `SessionEntry` fields (inactive fallback). ([Session Drawer Screen](../session-drawer.md))
- [ ] A common-room participant with `session` metadata produces a drawer row showing goal/state/agent/model with no `ListSessions` call for that row. ([Session Drawer Screen](../session-drawer.md), [LiveKit common room: owned project count](../livekit-participant-owned-projects.md))
- [ ] Background attachments persist until explicit disconnect. ([Session Drawer Screen](../session-drawer.md))
- [ ] Existing `ConnectionScreen` (`#/`) behaviour is unchanged. ([Web Terminal](../web-terminal.md))

## References

### Affected Features (Complete List)

- [Session Drawer Screen](../session-drawer.md) — primary.
- [Web Terminal](../web-terminal.md) — background terminal lifecycle, per-session room ownership.
- [LiveKit common room: owned project count](../livekit-participant-owned-projects.md) — `session` metadata key.

### Related Documentation

- New coder feature doc: `docs/ft/coder/session-participant-rpc.md`.
- Daemon PRD: `docs/ft/daemon/1-WIP/PRD-2026-07-12-fast-session-change.md`.
- Changeset: `docs/dev/1-WIP/2026-07-12-fast-session-change.md`.
- [LiveKit peer discovery (daemon)](../daemon/livekit-peer-discovery.md) — daemon participant identity model.
