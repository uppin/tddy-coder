# Changeset: Agent clones and the session-agent roster through ports

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #633
**Type**: Refactor (architecture)
**Stack**: #satellite 11/13. Base: `feature/satellite/forwarding-consumers` (#632).

## Initial Discovery

[2026-10-16-livekit-satellite-agent-clones-initial-discovery.md](./2026-10-16-livekit-satellite-agent-clones-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md](../todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md) | ⚠ DURING | Swapping the clone's transport is not the split that TODO asks for. Do not widen the module. |
| [2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md](../todo/2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md) | — unrelated | Behavioural gaps; transport unchanged. |

## Responsibility

`tddy-session-agents`' LiveKit uses, behind ports. Owns `RoomBroadcast`, `RoomAdmission` and
`RoomDirectory` (each with a null object) in `tddy-daemon-extensions`, the plugin's implementations
of them, the clone's switch to `RoomConnector` and `RoomTokenMinter`, and `session_room_name`'s move
to the kernel. Strikes `tddy-session-agents`.

## Boundaries

- Does not split `session_agent_clone.rs` (TODO above).
- Does not touch session-split (node 12) or the lifecycle and launch (node 13).
- Uses node 7's and node 4's ports and does not change them.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #624 peer-forwarding | `CommonRoom` over `PeerForwarder`, `is_connected` | Clone and provisioning forwards (`session_agent_clone.rs:475`, `svc_provision_agent_clone.rs:195`) | Change `CommonRoom` |
| #625 session-rooms | `SessionRooms::hosts` | `svc_ensure_session_room_for_agents.rs:27` asks the port | Change the port |
| #626 room-tokens | `RoomTokenMinter` | The clone's self-mint fallback (`session_agent_clone.rs:529`) | Change minting |
| #629 client-transports | `tddy-room-client` (`RoomConnector`, `RoomSession`) | The clone joins the session room through it | Change the client port |
| #632 forwarding-consumers | Kernel `peer_calls` (`forward_delete_session`, `forward_start_session_within`); `tddy_host_service::peer_routing::PeerRouting` | Provisioning forwards and routing | Re-add `*_via_livekit` names |

**As published (wave 2):** `RoomBroadcast`, `RoomAdmission`, `RoomDirectory` with null objects;
`tddy_daemon_kernel::peer_forwarding::session_room_name`; the plugin's impls (`SessionRoomRegistry` as
`RoomBroadcast`, `SessionAdmissionRegistry` as `RoomAdmission`, `RosterDirectory` over `RoomRoster`) —
all `todo!()` — with failing unit tests. **Shape correction:** `RoomAdmission` mirrors the registry it
wraps (synchronous `admit`/`revoke`/`is_admitted` returning `bool`, plus `revoke_all_for_session`);
the scoped join token stays with `first_admission_token`, not on the port. **Not yet published:** the
session-agents acceptance tests (`roster_ports.rs`, `clone_room_join.rs`) — they drive `AgentRoster`
paths whose fields change type in green. The ratchet strike waits on #623's `WorkspaceGraph`.

**Red pass (2026-10-17):** added `roster_broadcast::broadcast_roster_through` and
`session_agent_clone::clone_room_join` with two tests in `tests/roster_ports.rs`.
⚠ **Open:** `an_owning_daemon_is_admitted_before_its_agent_is_attached` needs a seam into the
attach flow (`AgentRoster` → `RoomAdmission`) that green introduces together with the field change;
it is not written yet.

**Green pass (2026-10-20):** the ports are real; `tddy-session-agents` names no LiveKit crate.
Deviations from the contract above, each forced by a call site:
- `RoomBroadcast::publish` is synchronous, so the plugin queues each snapshot on one ordered task
  (`SessionRoomRegistry::agents_outbox`); a task per publish would let snapshots overtake each other.
- `RoomDirectory::participants` answers `NotFound` for a room the server does not list (the roster's
  old behaviour), not an empty list.
- The first admission token is minted in the core through `RoomTokenMinter` and
  `ADMISSION_TOKEN_TTL` (moved to `tddy-daemon-extensions`), after `RoomAdmission::admit`; the room
  server URL comes from the `livekit:` block (`room_access::room_server_url`).
  `tddy_daemon_livekit::first_admission_token` has no caller left.
- `WorktreeSnapshot` (kernel `worktree_snapshot`) and `OpenedSessionRoom` (`tddy-daemon-extensions`)
  moved out of the plugin because `AgentHostCallbacks` names them; the plugin re-exports both.
- `AgentRoster` carries `room_directory`, `room_broadcast`, `room_tokens` and `room_connector`; the
  session host builds them from its registries (`handler_state::agent_roster`).

## Draft PR contract

```rust
// tddy-daemon-extensions/src/room_broadcast.rs
pub trait RoomBroadcast: Send + Sync { fn publish(&self, session_id: &str, topic: &str, payload: Vec<u8>) -> Result<(), Status>; }
pub struct NoRoomBroadcast;
// tddy-daemon-extensions/src/room_admission.rs
#[async_trait::async_trait]
pub trait RoomAdmission: Send + Sync {
    /// Admit `owning_instance_id` to `session_id`'s room; returns its scoped join token.
    async fn admit(&self, session_id: &str, owning_instance_id: &str) -> Result<String, Status>;
    fn revoke(&self, session_id: &str, owning_instance_id: &str);
    fn is_admitted(&self, session_id: &str, owning_instance_id: &str) -> bool;
}
pub struct NoRoomAdmission;
// tddy-daemon-extensions/src/room_directory.rs
#[async_trait::async_trait]
pub trait RoomDirectory: Send + Sync {
    async fn participants(&self, room: &str) -> Result<Vec<String>, Status>;
}
pub struct NoRoomDirectory;
// tddy-daemon-kernel: pub fn session_room_name(session_id: &str) -> String;
```

The exact admission and directory method sets follow the call sites in `agent_roster_state.rs`,
`agent_host_callbacks.rs` and `session_room_participants.rs`, and are fixed when wave 2 publishes the
surface. Bodies are `todo!()` until green.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** no. It consumes the behaviour of #624, #625, #626, #629 and #632.
**Concurrent with:** node 12 (split-transport)
**Blocks:** node 13 (lifecycle-launch), which deletes the offender list

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-daemon-kernel` (`session_room_name`),
`tddy-session-agents`, `tddy-session-lifecycle` (roster construction), `tddy-daemon` (wiring).

## Summary

The session-agent roster and its clones talk to "a room" through ports.

## Scope

- [x] `RoomBroadcast`, `RoomAdmission`, `RoomDirectory` + null objects
- [x] Plugin implementations over `BroadcastPublisher`, `SessionAdmissionRegistry`, `RoomRoster`
- [x] Clone on `RoomConnector` + `RoomTokenMinter`
- [x] `session_room_name` → kernel (plugin re-exports)
- [x] Ratchet: strike `tddy-session-agents`

## Technical Changes

### State A
- `session_agent_clone.rs:27-28` (`connect_client`, `ConnectedClient`, `BroadcastChannel`,
  `RpcClient`, `TokenGenerator`), `:275` (holds `Arc<livekit::Room>`), `:475` (`CommonRoom`),
  `:488-492` (`session_room_name`, `daemon_rpc_identity`), `:529` (self-mint).
- `roster_broadcast.rs:12,18` (`BroadcastPublisher`, `SESSION_AGENTS_TOPIC`).
- `agent_host_callbacks.rs:27-30,93-98`; `agent_roster_state.rs:15-18,38`
  (`RoomRoster`, `PeerRouting`, `SessionAdmissionRegistry`, `SessionRoomRegistry`).
- `session_room_participants.rs:3` (`RoomRoster`).
- `svc_provision_agent_clone.rs:17,100,147,203,302,316` (admission token, forwards, agents
  publisher, room name).
- Manifest: `tddy-daemon-livekit`, `tddy-livekit`, `livekit = "0.7"`, each documented (`Cargo.toml:18-42`).

### State B
Every LiveKit-facing need is one of the ports above, node 7's `RoomConnector`, node 4's minter or
node 3's rooms. The plugin implements them, and the daemon wires them into `AgentRoster`.

### Delta
As listed in Scope. The `AgentRoster` fields change from concrete plugin types to port trait
objects.

## Implementation Milestones
- [x] M1 Ports + plugin implementations
- [x] M2 Roster and admission on ports (in-memory tests green)
- [x] M3 Clone on `RoomConnector` (testkit suites green)
- [x] M4 Manifest; ratchet green

## Testing Plan

Roster and admission logic is tested against in-memory ports. The clone's join is tested against an
in-memory `RoomConnector`. The existing testkit-backed roster and clone suites (in
`tddy-session-lifecycle/tests`) stay the integration proof.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_changed_roster_is_broadcast_on_the_agents_topic_of_its_session_room` | `packages/tddy-session-agents/tests/roster_ports.rs` (new) | `RoomBroadcast::publish(session, SESSION_AGENTS_TOPIC, roster bytes)` |
| `an_owning_daemon_is_admitted_before_its_agent_is_attached` | same | `RoomAdmission::admit` precedes the forward to the owning peer |
| `a_hosted_clone_joins_the_session_room_and_waits_for_the_facilitating_daemon` | `packages/tddy-session-agents/tests/roster_ports.rs` | `RoomConnector::connect` join and peer identity |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Entry struck; both ratchet tests pass |

Unit: null objects in `tddy-daemon-extensions` (`with_nothing_wired_*`).

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Split from the original node 11.** One PR covering session-agents, session-split, lifecycle and
  agent-launch was too large to review. It is now cut by capability into 11 (agent roster and
  clones), 12 (split transport) and 13 (lifecycle and launch, which closes the ratchet).
- **Reuse node 7's client port for the clone.** A hosted clone *is* a room client of the
  facilitating daemon. That creates a real edge `n7 → n11`.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-agent-clones-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
