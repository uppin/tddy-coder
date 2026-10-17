# Changeset: Lifecycle and launch off the LiveKit plugin; the ratchet closes

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #635
**Type**: Refactor (architecture)
**Stack**: #satellite 13/13. Base: `feature/satellite/split-transport` (#634).

## Initial Discovery

[2026-10-16-livekit-satellite-lifecycle-launch-initial-discovery.md](./2026-10-16-livekit-satellite-lifecycle-launch-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ DURING | The host struct changes field types. It must not grow; builders go in their own `svc_*_wiring.rs` files, matching existing practice. |
| [2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md](../todo/2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md) | ✅ RESOLVED HERE (Rust side) | With every LiveKit capability behind a port wired once in runtime, "is LiveKit configured?" is answered once. #623 narrowed the record. This node closes the Rust half; the web half is #631's surface. |
| [2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites.md](../todo/2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites.md) | — unrelated | A plugin-internal dependency, allowed. |

## Responsibility

Switching the host and launch wiring onto the ports, closing the ratchet. Owns
`SessionReach::open_now` and `OpenedRoom` in the extensions crate; `DaemonSessionHost`'s
port-based construction; the move of `RoomsHostedByThisDaemon` into the plugin; the helper moves
(`session_type_is_facilitated_here` and `snapshot_worktree_within` → `tddy-agent-launch`,
`classify_start_session_peer_route` → removed in favour of the kernel's `classify_peer_route`); the
removal of lifecycle's plugin re-exports; the test helper's move to `tddy-livekit-testkit`; and
deleting `KNOWN_OFFENDERS`. Strikes `tddy-session-lifecycle` and `tddy-agent-launch`.

## Boundaries

- Changes no behaviour. Ports and implementations exist from earlier nodes.
- Does not add a cargo feature to the daemon binary (follow-up TODO).
- Does not split `connection_service.rs`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 extension-ports | `SessionReach`, `ReachableSessions` (incl. `expose_terminal`), the ratchet | Adds `open_now`; deletes the offender list | Change `session_reached`'s contract |
| #625 session-rooms | `SessionRooms` | Host and launch hold `dyn SessionRooms` | Change the port |
| #628 terminal-bridge | `TerminalExposure` | `expose_terminal` uses it; `SessionTerminalBridge` impl deleted | Change the port |
| #632 forwarding-consumers | `PeerRouting` in host-service; kernel `peer_calls` | Host constructor takes `PeerRouting`; launch forwards | Move routing again |
| #633 agent-clones | `RoomAdmission`, `RoomDirectory`, `RoomBroadcast` | Host builders; `AgentRoster` fields | Change the ports |
| #634 split-transport | `SplitRoomHosting`; moved `WorktreeSnapshot`, `RemoteSnapshotSource` | Host builders; lifecycle's snapshot impl | Change the ports |

**As published (wave 2):** `OpenedRoom` and `SessionReach::open_now` (both implementations `todo!()`)
with a failing unit test, and the ratchet's final form — `KNOWN_OFFENDERS` and its
`every_known_offender_…` test deleted, `only_plugins_and_wiring_name_a_livekit_crate` the closing
assertion (fails until every node is green; also waits on #623's `WorkspaceGraph`). **For green:**
`LiveKitSessionReach` still has #623's inherent `open_now(&ReachedSession, ..)` beside the new trait
method — fold the inherent one into the trait's. **Not yet published:** the port-based
`DaemonSessionHost::new` and its builders, `host_without_integrations.rs`, the delta-adapter and helper
moves, and the test-helper move — each changes every fixture's construction at once, so they land
together in green.

**Red pass (2026-10-17):** added `opened_session_room::require_opened_room` (the agent-roster
check over the port's `OpenedRoom`) with two tests. ⚠ **Open:**
`a_host_built_with_nothing_wired_serves_a_local_session` needs the port-based
`DaemonSessionHost::new`, which changes every fixture at once (green).
`the_activity_service_reads_deltas_through_the_plugins_store` is **blocked by a dependency cycle**:
the adapter can only move into `tddy-daemon-livekit` once `tddy-session-activity` stops depending on
the plugin, which is #625's green.

## Draft PR contract

```rust
// tddy-daemon-extensions
pub struct OpenedRoom { pub room: String, pub url: String, pub server_identity: String }
#[async_trait::async_trait]
pub trait SessionReach: Send + Sync {
    async fn session_reached(&self, session: ReachedSession, host: Arc<dyn ReachableSessions>) -> Result<(), Status>;
    /// Open the session's room now, for a caller that cannot proceed without it. `Ok(None)`: no integration.
    async fn open_now(&self, session: ReachedSession, host: Arc<dyn ReachableSessions>) -> Result<Option<OpenedRoom>, Status>;
}

// tddy-session-lifecycle
impl DaemonSessionHost {
    pub fn new(config, sessions_base_for_user, tddy_data_dir, user_resolver, spawn_client,
               peer_routing: tddy_host_service::peer_routing::PeerRouting,
               presenter_event_sink, claude_cli_manager) -> Self;
    pub fn with_session_rooms(self, Arc<dyn SessionRooms>) -> Self;
    pub fn with_terminal_exposure(self, Arc<dyn TerminalExposure>) -> Self;
    pub fn with_room_admission(self, Arc<dyn RoomAdmission>) -> Self;
    pub fn with_room_directory(self, Arc<dyn RoomDirectory>) -> Self;
    pub fn with_room_broadcast(self, Arc<dyn RoomBroadcast>) -> Self;
    pub fn with_split_rooms(self, Arc<dyn SplitRoomHosting>) -> Self;
}

// tddy-daemon-livekit
pub struct RoomsHostedByThisDaemon { .. }   // impl tddy_session_activity::SessionDeltaStores

// tddy-livekit-testkit
pub async fn serve_daemon_rpc_participant(..);   // moved from tddy-session-lifecycle::test_util
```

Bodies are `todo!()` until green; the failing tests below are published with the surface.

## Green wave

**Wave:** 5 of 5
**Greenable independently:** no. It is the closing node and consumes every port.
**Concurrent with:** none
**Blocks:** none

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-session-lifecycle`, `tddy-agent-launch`, `tddy-daemon-livekit`,
`tddy-daemon-kernel`, `tddy-livekit-testkit`, `tddy-daemon` (runtime + tests), `tddy-daemon-rpc`,
`tddy-desktop` (through runtime), and every crate importing a lifecycle re-export of a plugin module.

## Summary

The core is LiveKit-free. LiveKit lives in its own crates and the daemon's wiring.

## Scope

- [ ] `OpenedRoom` + `SessionReach::open_now`; agent-roster paths switched (`svc_ensure_session_room_for_agents.rs`, `require_opened_session_room`)
- [ ] `DaemonSessionHost::new` + builders on ports; runtime wires the plugin; fixtures updated
- [ ] `RoomsHostedByThisDaemon` → plugin
- [ ] Helpers moved; `classify_start_session_peer_route` / `StartSessionPeerRoute` aliases removed
- [ ] Lifecycle `lib.rs:86-92` re-exports removed; callers repointed
- [ ] `serve_daemon_rpc_participant` → `tddy-livekit-testkit`
- [ ] Ratchet: `KNOWN_OFFENDERS` and its test deleted; final assertion

## Technical Changes

### State A
- Lifecycle `lib.rs:86-92` re-exports `common_room_supervisor`, `livekit_peer_discovery`,
  `livekit_rooms_stream`, `livekit_service`, `session_room`, `peer_routing` and
  `session_admission_service`. `connection_service.rs:9-10` (`RoomRoster`, `ActivityDelta`), `:163`
  (registry field), `:411` (chunk constant), `:508-512` (re-exports).
- `svc_host_builders.rs:5-8,35,55`: the constructor takes `Option<LiveKitDiscoveryHandles>` and calls
  `room_roster_from_config`.
- `session_room_opening.rs` (`ensure_session_room` → `ensure_open`); `svc_agent_host_ports.rs:12-14`;
  `terminal_bridge_impl.rs:8` (`impl SessionTerminalBridge`).
- `svc_activity_ports.rs:125-165`: `RoomsHostedByThisDaemon` implements activity's
  `SessionDeltaStores` (`tddy-session-activity/src/service.rs:127`) by translating plugin
  `DeltaScope`, `DeltaLookupError` and `ActivityDelta`.
- `src/test_util.rs:355`: `serve_daemon_rpc_participant` uses `LiveKitParticipant::connect`. It is
  used by `tddy-daemon/tests/{session_agent_remote,session_attach_cross_host,split_session_resume,
  remote_managed_worktree_cross_host,session_room_cross_host}_acceptance.rs`.
- agent-launch: `session_coordinate_handlers.rs:137,237`; `launch_ports.rs:25,118`;
  `svc_start_session_core.rs:113,121` (`StartSessionPeerRoute` is an alias of the kernel's
  `PeerRoute`, `livekit_peer_discovery.rs:390`).

### State B
Both crates' manifests drop `tddy-daemon-livekit`, `tddy-livekit` and `livekit`. `tddy-daemon`'s
runtime is the only place that turns the plugin's types into the host's ports.

### Delta
As listed in Scope.

## Implementation Milestones
- [ ] M1 `open_now` and the agent-roster paths
- [ ] M2 Host construction on ports; runtime and fixtures
- [ ] M3 Adapter and helper moves; re-exports removed
- [ ] M4 Test helper moved; ratchet closed

## Testing Plan

Node 1's acceptance suites (local connect, `OpenSessionRoom`, the no-plugin host) run against the new
constructor. The agent-roster suites cover `open_now`. The ratchet's final form is the closing
assertion. Scoped runs for every touched package, and CI for the whole workspace, which this node
changes widely.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_host_built_with_nothing_wired_serves_a_local_session` | `packages/tddy-session-lifecycle/tests/host_without_integrations.rs` (new) | Every port at its null object: start, connect locally, resume and delete succeed |
| `a_remote_agent_is_refused_as_before_when_no_integration_hosts_rooms` + `a_remote_agent_proceeds_once_the_room_is_open` | `packages/tddy-session-agents/tests/opened_room.rs` (the agent-roster check over `OpenedRoom`) | Agent-roster path calls `open_now`; `Ok(None)` → today's precondition message |
| `only_plugins_and_wiring_name_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` (rewritten; present) | No `KNOWN_OFFENDERS`; every crate naming a LiveKit crate is in `PLUGINS ∪ WIRING` |
| `the_activity_service_reads_deltas_through_the_plugins_store` | `packages/tddy-daemon-livekit/tests/session_delta_stores.rs` (new) | The moved adapter maps the scope and lookup errors as before |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **`open_now` on `SessionReach`, not a second port.** The agent-roster paths need exactly what
  `OpenSessionRoom` needs: open now and fail loudly. One port with two entry points keeps one
  implementation.
- **The delta adapter moves rather than a new port being added.** The activity service already
  declares `SessionDeltaStores`; the adapter was in the wrong crate.
- **The last node deletes the list.** With every crate struck, the list would be empty, and an empty
  allow-list is noise. The final assertion is stricter.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-16-livekit-satellite-lifecycle-launch-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
