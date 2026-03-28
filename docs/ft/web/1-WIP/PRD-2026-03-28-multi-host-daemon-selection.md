# PRD: Multi-Host Daemon Selection in tddy-web

**Status:** Draft
**Created:** 2026-03-28

## Summary

Add a per-project "Host" dropdown to `ConnectionScreen` in `tddy-web` so users can choose which daemon (host machine) runs their session. Daemons discover each other via the shared LiveKit common room. The gateway daemon (the one serving the web bundle) routes `StartSession` to the selected peer daemon over LiveKit RPC. Session tables display which host each session is running on.

## Background

The `feature/multi-host-daemon-selection` branch already has daemon-side foundation work:
- `daemon_instance_id` in daemon config → LiveKit identity `daemon-{instance_id}-{session_id}`
- `multi_host.rs` with `EligibleDaemonSource` trait and `StubEligibleDaemonSource`
- `ProjectData.host_repo_paths` for per-host repo paths
- Multi-host-safe session deletion (returns `FailedPrecondition` when session directory is missing on this daemon)

What's missing: the web UI has no way to list available hosts or select one when starting a session. The daemon has no real peer discovery (only a stub returning the local daemon). There is no daemon-to-daemon RPC for delegating `StartSession` to a peer.

## Affected Features

- [web/web-terminal.md](../web-terminal.md) — ConnectionScreen gains Host dropdown and host column in session tables
- [daemon/project-concept.md](../../daemon/project-concept.md) — `host_repo_paths` already exists; `StartSession` gains host routing
- [daemon/PRD-2026-03-19-tddy-daemon.md](../../daemon/1-WIP/PRD-2026-03-19-tddy-daemon.md) — daemon gains peer discovery and cross-host session spawn

## Requirements

### 1. Daemon Discovery via LiveKit Common Room

Each daemon joins the shared LiveKit common room (already configured via `livekit.common_room` in daemon YAML) with a participant identity that encodes its instance ID, e.g. `daemon-ctrl-{instance_id}`. The gateway daemon discovers peers by listing room participants whose identity matches the `daemon-ctrl-*` pattern.

- `daemon_instance_id` defaults to hostname (already implemented via `local_daemon_instance_id()`)
- Each daemon publishes metadata (label, capabilities) as LiveKit participant metadata JSON
- Discovery is live — daemons appearing/disappearing in the room are reflected in UI refresh

### 2. `ListEligibleDaemons` RPC

New method on `ConnectionService`:

```protobuf
rpc ListEligibleDaemons(ListEligibleDaemonsRequest) returns (ListEligibleDaemonsResponse);

message ListEligibleDaemonsRequest {
  string session_token = 1;
}
message EligibleDaemonEntry {
  string instance_id = 1;
  string label = 2;
  bool is_local = 3;
}
message ListEligibleDaemonsResponse {
  repeated EligibleDaemonEntry daemons = 1;
}
```

- Returns all daemons visible in the common room (including self, marked `is_local: true`)
- When no common room is configured or only one daemon exists, returns a single entry (local)
- Replaces `StubEligibleDaemonSource` with a LiveKit-based implementation

### 3. `StartSession` Host Routing

`StartSessionRequest` gains an optional `daemon_instance_id` field:

```protobuf
message StartSessionRequest {
  string session_token = 1;
  string tool_path = 2;
  string project_id = 3;
  string agent = 4;
  string daemon_instance_id = 5;  // NEW: target host; empty = local
}
```

- When empty or matching the local daemon's instance ID → spawn locally (current behavior)
- When targeting a peer → the gateway daemon sends a spawn request to the peer via LiveKit RPC in the common room, targeting the peer's `daemon-ctrl-{instance_id}` participant identity
- The peer daemon spawns the process and returns session metadata (session ID, LiveKit room, server identity)
- The gateway daemon relays the response back to the web client

### 4. Session Host Display

`SessionEntry` gains a `daemon_instance_id` field:

```protobuf
message SessionEntry {
  string session_id = 1;
  string created_at = 2;
  string status = 3;
  string repo_path = 4;
  uint32 pid = 5;
  bool is_active = 6;
  string project_id = 7;
  string daemon_instance_id = 8;  // NEW: which host owns this session
}
```

- Session tables in the web UI show a "Host" column
- Value is the daemon label (e.g. "workstation-1") or "local" for single-daemon setups

### 5. Per-Project Host Dropdown in tddy-web

`ConnectionScreen` → `ProjectSessionOptions` gains a "Host" dropdown alongside Tool and Backend:

- Populated by `ListEligibleDaemons` response
- Defaults to the local daemon (or the first entry)
- Selected value is passed as `daemon_instance_id` in `StartSession`
- When only one daemon is available, the dropdown can be hidden or shown as disabled with the single option
- Per-session / per-connection setting (not stored on the project), same as Tool and Backend

### 6. Single-Daemon Backward Compatibility

When `common_room` is not configured or only one daemon is present:
- `ListEligibleDaemons` returns one entry (local)
- `StartSession` without `daemon_instance_id` spawns locally
- No behavioral change from current single-daemon experience
- Host dropdown shows one option or is hidden

## Success Criteria

1. `ListEligibleDaemons` returns all daemons visible in the shared common room
2. Host dropdown appears per-project in ConnectionScreen, populated from `ListEligibleDaemons`
3. `StartSession` with a `daemon_instance_id` spawns the session on the target peer
4. Session tables show which host each session is running on
5. Single-daemon setups work identically to before (no regression)
6. Daemon discovery is live (peer joins/leaves common room → reflected on next `ListEligibleDaemons` call)

## Scope Boundaries

**In scope:**
- `ListEligibleDaemons` RPC (proto + daemon implementation)
- LiveKit-based daemon discovery (replace stub with real implementation)
- `StartSession` host routing via LiveKit RPC to peer daemon
- `daemon_instance_id` on `StartSessionRequest` and `SessionEntry`
- Host dropdown in `ConnectionScreen` per project
- Host column in session tables
- Backward compatibility for single-daemon setups

**Out of scope (future):**
- `ConnectSession` / `ResumeSession` cross-host routing (sessions are connected via LiveKit regardless of which daemon spawned them — the `livekit_server_identity` already handles this)
- Load balancing / automatic host selection
- Host health monitoring / failover
- Per-project host affinity (persistent host assignment)
- Cross-host session migration
