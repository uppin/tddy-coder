# Changeset: Split-placement transport through ports

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #634
**Type**: Refactor (architecture)
**Stack**: #satellite 12/13. Base: `feature/satellite/agent-clones` (#633).

## Initial Discovery

[2026-10-16-livekit-satellite-split-transport-initial-discovery.md](./2026-10-16-livekit-satellite-split-transport-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` | ⚠ DURING | The room-hosting block (`:296-337`) becomes one port call, which should lower the complexity. Re-measure at wrap. |
| `packages/tddy-session-split/docs/code-issues/complexity-split-claude-cli-start-start-split-claude-cli-session.md` | ⚠ DURING | The forward call changes its path only. |
| `packages/tddy-session-split/docs/code-issues/complexity-svc-paired-codebase-teardown-delete-paired-codebase-session.md` | ⚠ DURING | `is_connected()` (#624) and the kernel forward (#632). The function's logic is unchanged. |
| [2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md](../todo/2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md) | — unrelated | Behavioural; transport unchanged. |

## Responsibility

Split placement's LiveKit needs, behind ports. Owns `SplitRoomHosting`, `SplitAgentJoin` and
`NoSplitRooms`; the move of `SessionTokenMinter`, `RemoteSnapshotSource` and `WorktreeSnapshot` into
`tddy-daemon-extensions` (with plugin re-exports); the plugin's `SplitRoomHosting` implementation;
and session-split's switch. Strikes `tddy-session-split`.

## Boundaries

- Does not change whether split placement needs LiveKit. It does, and loudly.
- Does not touch session-agents (#633) or lifecycle and launch (node 13). It does not move the
  activity-delta types (node 13).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #624 peer-forwarding | `CommonRoom::is_connected` | Teardown (`svc_paired_codebase_teardown.rs:136`) | Change `CommonRoom` |
| #626 room-tokens | `RoomTokenMinter` | The plugin's `SplitRoomHosting::agent_join` mints through it | Change minting |
| #632 forwarding-consumers | Kernel `forward_start_session_within`, `forward_delete_session`; `tddy_host_service::peer_routing::PeerRouting` | Split start and teardown forwards; `split_ports.rs:18` | Re-add the plugin names |
| #633 agent-clones | `session_room_name` in the kernel | Split room name (`split_claude_cli_start.rs:85`) | Change the helper |

**As published (wave 2):** `WorktreeSnapshot`, `SessionTokenMinter` and `RemoteSnapshotSource` are
**moved** into `tddy_daemon_extensions::split_rooms` (verbatim; `WorktreeSnapshot::tracked_diff_differs`
moved with its type and became `pub`, since an inherent method cannot stay behind in the plugin) and
re-exported from `tddy_daemon_livekit::session_room`, so every caller compiles unchanged — the plugin's
lib suite is 77 passing, its only 6 failures being #623's own red tests. New: `SplitRoomHosting`,
`SplitAgentJoin`, `NoSplitRooms` (`todo!()`) with a failing unit test. **Not yet published:** the plugin's
`SplitRoomHosting` impl and session-split's acceptance tests (`split_room_hosting_port.rs`) — they drive
`SplitSessions` paths that switch in green. The ratchet strike waits on #623's `WorkspaceGraph`.

**Red pass (2026-10-17):** added `split_session::split_remote_tool_env_from_join` and
`tests/split_room_hosting_port.rs` (`a_split_agent_is_spawned_with_the_join_the_port_issued`); the
refusal row is the `NoSplitRooms` unit test. ⚠ **Open:** `a_split_start_hosts_the_room_over_the_remote_checkout`
needs a seam into `spawn_split_agent` that green introduces with the `SplitSessions` field change.

## Draft PR contract

```rust
// tddy-daemon-extensions/src/split_rooms.rs
pub struct WorktreeSnapshot { /* moved verbatim from tddy_daemon_livekit::session_room */ }
pub trait SessionTokenMinter: Send + Sync { fn mint(&self) -> String; }          // moved
#[async_trait::async_trait]
pub trait RemoteSnapshotSource: Send + Sync {                                     // moved
    async fn snapshot(&self, session_token: &str, codebase_session_id: &str, codebase_instance_id: &str)
        -> Result<WorktreeSnapshot, Status>;
}
pub struct SplitAgentJoin { pub url: String, pub room: String, pub token: String }
#[async_trait::async_trait]
pub trait SplitRoomHosting: Send + Sync {
    /// Open `session_id`'s room measuring a checkout held by `codebase_instance_id`.
    async fn host_remote_worktree(&self, session_id: &str, session_dir: &Path,
        codebase_session_id: &str, codebase_instance_id: &str,
        snapshots: Arc<dyn RemoteSnapshotSource>, minter: Arc<dyn SessionTokenMinter>,
        services: Arc<dyn ReachableSessions>) -> Result<(), Status>;
    /// The room join a split agent is spawned with.
    fn agent_join(&self, room: &str, identity: &str, ttl: Duration) -> Result<SplitAgentJoin, Status>;
}
/// No integration: split placement is refused with today's precondition message.
pub struct NoSplitRooms;
```

Bodies are `todo!()` until green; the failing tests below are published with the surface.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** no. It consumes the behaviour of #624, #626 and #632.
**Concurrent with:** node 11 (#633)
**Blocks:** node 13

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-session-split`, `tddy-session-lifecycle`
(`SplitSessions` construction, `RemoteSnapshotSource` impl at `svc_agent_roster_wiring.rs:12`),
`tddy-daemon` (wiring).

## Summary

A split session reaches its remote checkout's room through a port, so session-split no longer
depends on LiveKit.

## Scope

- [ ] Move `WorktreeSnapshot`, `SessionTokenMinter`, `RemoteSnapshotSource` → extensions (plugin re-exports)
- [ ] `SplitRoomHosting` + `SplitAgentJoin` + `NoSplitRooms`
- [ ] Plugin `SplitRoomHosting` over registry + `RemoteCheckout` + `RoomTokenMinter`
- [ ] session-split on the port and kernel forwards
- [ ] Ratchet: strike `tddy-session-split`

## Technical Changes

### State A
- `split_session.rs:185-193` mints the split agent's token (`SPLIT_AGENT_TOKEN_TTL`); `:275-297`
  `SplitLiveKit::from_config` reads `livekit_common_room_connect_strings`; `:295` uses the kernel's
  `daemon_rpc_identity`.
- `agent_credentials.rs:95-115` `RoomPollTokenMinter` implements the plugin's `SessionTokenMinter`.
- `svc_spawn_split_agent.rs:296-337` builds `RemoteCheckout::new(..)` and calls
  `session_rooms.open_measured_by(DaemonRoomHosting{..}.for_remote_worktree(..), roster, source)`.
- `split_ports.rs:18-21,97` (`PeerRouting`, `RemoteSnapshotSource`, `SessionRoomRegistry`,
  `WorktreeSnapshot`); `svc_paired_codebase_teardown.rs:29,39,136,144`;
  `split_claude_cli_start.rs:85,95`.
- Plugin: `session_room.rs:2019` `RemoteCheckout`, `:2047` `SessionTokenMinter`, `:2056`
  `RemoteSnapshotSource`, `:81` `WorktreeSnapshot`.
- Manifest `:18` `tddy-daemon-livekit`, `:22` `tddy-livekit`.

### State B
Session-split asks `SplitRoomHosting` for both needs and implements the moved traits from the
extensions crate.

### Delta
As listed in Scope.

## Implementation Milestones
- [ ] M1 Trait and type moves (re-exports keep everything compiling)
- [ ] M2 Port + plugin implementation
- [ ] M3 Split switched; suites green; ratchet green

## Testing Plan

Split-start logic is tested against an in-memory `SplitRoomHosting`: the join it asks for and the
refusal with `NoSplitRooms`. The existing split suites (`tddy-daemon/tests/split_session_*`, and the
lifecycle's split tests) remain the testkit-backed integration proof.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_split_start_hosts_the_room_over_the_remote_checkout` | `packages/tddy-session-split/tests/split_room_hosting_port.rs` (new) | `host_remote_worktree(session, dir, codebase session, codebase instance, ..)` with the verified-caller minter |
| `a_split_agent_is_spawned_with_the_join_the_port_issued` | same | `agent_join(room, split identity, SPLIT_AGENT_TOKEN_TTL)` flows into the agent's env |
| `with_nothing_wired_split_placement_is_refused_as_before` | `packages/tddy-daemon-extensions/src/split_rooms.rs` (unit) | `NoSplitRooms` produces `FailedPrecondition` "cannot place a session's codebase on another daemon: …" |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Entry struck; both ratchet tests pass |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Move the LiveKit-free traits instead of wrapping them.** `SessionTokenMinter` and
  `RemoteSnapshotSource` are ports that happen to be defined in the plugin. Moving them keeps one
  definition.
- **Split placement still requires LiveKit.** `NoSplitRooms` keeps today's loud refusal; this is not
  a fallback.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-split-transport-initial-discovery.md`)
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
