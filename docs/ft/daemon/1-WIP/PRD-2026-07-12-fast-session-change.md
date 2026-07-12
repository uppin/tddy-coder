# Fast Session Change — Daemon PRD

**Date**: 2026-07-12
**PRD Type**: Requirement Update

## Affected Features

**CRITICAL**: List ALL feature documents affected by this PRD:

- **Primary Feature**: [Terminal Sessions](../terminal-sessions.md) — clarify the bootstrap/directory boundary: the daemon remains the authority for `StartSession` / `ConnectSession` / `ResumeSession` and directory RPCs, while session-scoped `ConnectionService` methods (tools, terminal control, VNC, screen-sharing) move to the coder's own LiveKit participant; `DeleteSession` / `SignalSession` stay **daemon-direct** (the web calls the daemon participant; the coder does not relay them).

## Summary

The daemon stops being the hop for session-scoped `ConnectionService` calls on LiveKit-backed (tddy-coder) sessions. Once a coder process is running and joined as `daemon-{instanceId}-{sessionId}`, the web calls tools / terminal-control / VNC / screen-sharing on that participant directly. The daemon keeps the bootstrap (`StartSession`, `ConnectSession`, `ResumeSession`) and directory (`ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`, `ListProjectBranches`) surface, and it keeps serving `DeleteSession` / `SignalSession` **directly** — the web routes them to `daemon-{instanceId}` (not through the coder participant), so lifecycle control still works when the coder is stuck, and so unattached-session management from the sessions list still works.

## Background

Routing every session-scoped RPC through the daemon participant couples the session screen to the daemon and adds a hop. With the coder serving its own session surface, the daemon's role narrows to directory authority and process lifecycle. `DeleteSession` / `SignalSession` must still terminate at the daemon (it owns the spawned process and the `.session.yaml` record), so the web calls them **directly** on `daemon-{instanceId}` — not relayed through the coder. This keeps lifecycle control available even when the coder participant is unresponsive.

## Proposed Changes

### What's Changing

- **Bootstrap / directory boundary (clarified).** The daemon participant (`daemon-{instanceId}`) continues to serve: `StartSession`, `ConnectSession`, `ResumeSession`, `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`, `ListProjectBranches`. These are the calls the web makes **before/without** an attached session participant.
- **Inspector data for sessions with no LiveKit participant.** `SessionEntry` gains `bytes_in` / `bytes_out` / `last_data_received_at` fields. The daemon populates these in `ListSessions` from the `GrpcSessionTerminal` traffic meter for claude-cli / cursor-cli / workspace sessions it owns (live counters), and reports zero / empty for tddy-coder sessions that have no LiveKit participant (stopped). The web inspector renders these when no per-session live runtime exists (see the web PRD's dual-source inspector section).
- **Session-scoped surface delegated.** `ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`, `WatchTerminalControl`, VNC, and screen-sharing for a LiveKit-backed session are served by the coder's participant (`daemon-{instanceId}-{sessionId}`), not the daemon. The daemon still serves these for non-LiveKit (claude-cli / cursor-cli / workspace) sessions where no coder participant exists.
- **`DeleteSession` / `SignalSession` daemon-direct contract.** The web calls these directly on the daemon participant (`daemon-{instanceId}`) with the caller's `session_token`. The daemon validates the token (GitHub user → OS user → session ownership) exactly as it does today, performs process teardown / signalling, updates `.session.yaml`, and returns the result. Daemon errors surface verbatim to the web caller. The daemon serves these whether or not the web is also attached to the coder participant; the coder is not involved.

### What's Staying the Same

- Process lifecycle ownership stays with the daemon: it spawns and tears down the coder process and owns the `.session.yaml` record.
- `session_list_enrichment.rs` (`SessionListStatusDisplay`) keeps enriching `SessionEntry` from disk for inactive and directory-listed sessions.
- Auth model is unchanged: `session_token` → GitHub user → OS user → session ownership, validated at the daemon for every `DeleteSession` / `SignalSession` (now always daemon-direct).
- claude-cli / cursor-cli / workspace sessions keep their existing daemon-served `ConnectionService` path.

## Impact Analysis

### Technical Impact

- **tddy-daemon**: no method is removed from the daemon; `DeleteSession` / `SignalSession` remain daemon-direct (the web is the only caller now; the coder no longer relays). Directory/bootstrap methods unchanged.
- **tddy-coder**: new session-scoped `ConnectionService` server for tools/terminal-control + metadata publisher; no relay client (delete/signal are daemon-direct) (see `docs/ft/coder/session-participant-rpc.md`).
- **tddy-web**: session-scoped RPCs retarget to the session participant; bootstrap/directory RPCs stay on the daemon (see the web PRD).

### User Impact

- Deleting or signalling an attached LiveKit session still works and still terminates at the daemon; the path is one hop longer but behaviour is identical.
- The sessions list's per-row delete/signal from an unattached row still calls the daemon directly.

## Implementation Plan

1. Document the daemon-direct delete/signal contract in `terminal-sessions.md` (this PRD's wrap target).
2. Keep `connection_service.rs` `DeleteSession` / `SignalSession` auth and teardown unchanged; add a contract test asserting a direct web call with a valid `session_token` clears auth and reaches session processing (regression guard for the daemon-direct path).
3. Coordinate with the coder (no relay) and web (retargeting + delete/signal daemon-direct) PRDs.

## Acceptance Criteria

- [ ] The daemon serves `StartSession` / `ConnectSession` / `ResumeSession` and all directory RPCs on `daemon-{instanceId}`. ([Terminal Sessions](../terminal-sessions.md))
- [ ] The daemon serves `DeleteSession` / `SignalSession` **directly** to the web (called on `daemon-{instanceId}`) with the caller's `session_token`; the coder is not on the path. ([Terminal Sessions](../terminal-sessions.md))
- [ ] Daemon errors from a `DeleteSession` / `SignalSession` surface verbatim to the web caller. ([Terminal Sessions](../terminal-sessions.md))
- [ ] Non-LiveKit (claude-cli / cursor-cli / workspace) sessions' `ConnectionService` path is unchanged. ([Terminal Sessions](../terminal-sessions.md))

## References

### Affected Features (Complete List)

- [Terminal Sessions](../terminal-sessions.md) — primary.

### Related Documentation

- Coder feature doc: `docs/ft/coder/session-participant-rpc.md`.
- Web PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md`.
- Changeset: `docs/dev/1-WIP/2026-07-12-fast-session-change.md`.
- [LiveKit peer discovery (daemon)](../daemon/livekit-peer-discovery.md) — daemon participant identity and routing.
