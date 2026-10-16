# Changeset: Session rooms through an extension port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #625
**Type**: Refactor (architecture)
**Stack**: #satellite 3/13. Base: `feature/satellite/peer-forwarding` (#624).

## Initial Discovery

[2026-10-16-livekit-satellite-session-rooms-initial-discovery.md](./2026-10-16-livekit-satellite-session-rooms-initial-discovery.md)

## Responsibility

The *close / hosts* face of the session-room registry. Owns `SessionRooms` and `NoSessionRooms` in
`tddy-daemon-extensions`, the plugin's `impl SessionRooms for SessionRoomRegistry`, and the switch
of `tddy-session-activity`, `tddy-telegram-control` and `tddy-worktree-service` onto the port.
Strikes `tddy-session-activity` and `tddy-telegram-control`.

## Boundaries

- Does not touch room *opening* (`ensure_open`, `open_measured_by`), activity publishing, delta
  stores, or agents publishers.
- Leaves `Arc<SessionRoomRegistry>` fields in `tddy-agent-launch` (`launch_ports.rs:114`),
  `tddy-session-agents` (`agent_host_callbacks.rs:93`, `agent_roster_state.rs:38`),
  `tddy-session-split` (`split_ports.rs:97`) and `tddy-session-lifecycle` (`connection_service.rs:163`)
  as they are. Those are node 11. `close_session_room(&registry, id)` keeps compiling through the
  plugin's impl.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | `tddy-daemon-extensions` crate; ratchet list | Adds `session_rooms.rs`; strikes two entries | Change `SessionReach` or the status board |
| #624 `feature/satellite/peer-forwarding` | Nothing consumed; it is the line's predecessor only | — | Touch `PeerForwarder` or `CommonRoom` |

**As published (wave 2):** the port, its null object, the plugin's impl, `close_session_room(&dyn
SessionRooms, ..)` and telegram's field/builder are on the surface. The worktree service keeps its
own `WorktreeRoomCloser` until green: re-aliasing it to the wider port in the contract would break
the runtime's `SessionRoomWorktreeCloser` adapter, which green deletes together with the switch.
The ratchet assertions additionally wait on #623's `WorkspaceGraph`.

## Draft PR contract

```rust
// tddy-daemon-extensions/src/session_rooms.rs
/// The rooms this daemon hosts for its sessions, as code outside the integration sees them.
pub trait SessionRooms: Send + Sync {
    /// Whether this daemon currently hosts `session_id`'s room.
    fn hosts(&self, session_id: &str) -> bool;
    /// Stop hosting `session_id`'s room, if one is hosted.
    fn close(&self, session_id: &str);
    /// Stop hosting every room measuring `worktree`, before the checkout is removed.
    fn close_for_worktree(&self, worktree: &Path);
}
/// No integration hosting rooms: nothing is hosted, closing is a no-op.
pub struct NoSessionRooms;

// tddy-daemon-livekit: impl SessionRooms for SessionRoomRegistry

// tddy-session-activity
pub fn close_session_room(rooms: &dyn SessionRooms, session_id: &str);
// tddy-telegram-control
pub fn with_session_rooms(self, rooms: Arc<dyn SessionRooms>) -> Self;
// tddy-worktree-service: `pub use tddy_daemon_extensions::{NoSessionRooms, SessionRooms as WorktreeRoomCloser};`
// (`with_session_rooms(Arc<dyn SessionRooms>)` unchanged in shape)
```

Bodies are `todo!()` until green. The failing tests below are published with the surface.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes. Its tests drive the port's null object, the real registry through
the port, and the deletion helper; none needs another wave-2 node's behaviour.
**Concurrent with:** nodes 2 and 4–9
**Blocks:** node 11 (agent-clones), which moves the remaining registry holders onto ports

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-session-activity`, `tddy-telegram-control`,
`tddy-worktree-service`, `tddy-daemon` (runtime wiring).

## Summary

Closing a session's room stops requiring the LiveKit plugin crate.

## Scope

- [ ] `SessionRooms` + `NoSessionRooms` in `tddy-daemon-extensions`
- [ ] `impl SessionRooms for SessionRoomRegistry` (plugin)
- [ ] `close_session_room(&dyn SessionRooms, ..)`; telegram control field and builder
- [ ] Worktree service onto the shared port; `SessionRoomWorktreeCloser` deleted from runtime
- [ ] Manifests: session-activity, telegram-control drop `tddy-daemon-livekit`
- [ ] Ratchet: strike `tddy-session-activity`, `tddy-telegram-control`

## Technical Changes

### State A
- `tddy-session-activity/src/session_deletion.rs:225` `close_session_room(rooms:
  &tddy_daemon_livekit::session_room::SessionRoomRegistry, id)` calls `rooms.close(id.trim())`.
  Doc links at `:217` and `:222` point at `crate::session_room` paths that do not exist in this crate.
- `tddy-telegram-control/src/telegram_session_control/mod.rs:247` field `session_rooms:
  Arc<SessionRoomRegistry>`, default `:346`, builder `:352-356`; used at
  `chaining_and_listing.rs:408`.
- `tddy-worktree-service/src/service.rs:44-56` `WorktreeRoomCloser { close_for_worktree }` +
  `NoSessionRooms`; field `:75`, builder `:102`, called `:412`.
- `tddy-daemon/src/runtime.rs:333-339` `SessionRoomWorktreeCloser` adapter; `:1091` worktree
  service, `:1773` telegram harness.
- Registry (`tddy-daemon-livekit/src/session_room.rs`): `hosts` `:1774` and `contains` `:1795` are
  the same check; `close` `:1800`; `close_for_worktree` `:1854`.

### State B
The three crates hold `dyn SessionRooms`. The daemon hands each the one shared registry as
`Arc<dyn SessionRooms>`. The plugin's impl delegates `hosts` to `contains`, and the other two
methods to the registry methods of the same name.

### Delta
| Package | Change |
|---|---|
| `tddy-daemon-extensions` | `src/session_rooms.rs`, `lib.rs` |
| `tddy-daemon-livekit` | `impl SessionRooms for SessionRoomRegistry` (`session_room.rs` or a small sibling module, given the oversized-file issue) |
| `tddy-session-activity` | `session_deletion.rs`; `Cargo.toml` −`tddy-daemon-livekit` +`tddy-daemon-extensions` |
| `tddy-telegram-control` | `telegram_session_control/mod.rs`; `Cargo.toml` likewise |
| `tddy-worktree-service` | `service.rs`, `lib.rs` re-export; `Cargo.toml` +`tddy-daemon-extensions` |
| `tddy-daemon` | `runtime.rs` (adapter removed) |
| ratchet test | strike two entries |

## Implementation Milestones
- [ ] M1 Port and null object
- [ ] M2 Plugin impl; registry-through-port tests green
- [ ] M3 Consumers switched; manifests trimmed; ratchet green

## Testing Plan

The port is pinned at its two implementations: the null object (unit) and the real registry
(plugin integration test, no LiveKit server needed, because closing an unhosted room is the ordinary
case). The deletion helper is tested over the null object and a recording fake. The ratchet asserts
the dependency outcome.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_registry_hosting_no_room_reports_none_hosted_and_closes_quietly` | `packages/tddy-daemon-livekit/tests/session_rooms_port.rs` (new) | The real `SessionRoomRegistry` as `&dyn SessionRooms`: `hosts` false, `close` and `close_for_worktree` no-ops |
| `closing_a_deleted_sessions_room_asks_the_rooms_to_close_that_session_once` | `packages/tddy-session-activity/tests/session_room_closing.rs` (new) | `close_session_room` trims the id and calls `close` exactly once on a recording `SessionRooms` |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Two entries struck; both ratchet tests pass |

Unit (`tddy-daemon-extensions/src/session_rooms.rs`): `with_nothing_wired_no_session_room_is_hosted`,
`with_nothing_wired_closing_is_a_no_op`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Generalise `WorktreeRoomCloser` rather than add a sibling.** Two ports for "stop hosting rooms"
  would invite a third. The worktree service keeps its old names through a re-export, so its call
  sites do not move.
- **`hosts` only, not `contains` as well.** The registry has both with the same body; the port names
  the one that states intent.
- **No server-backed test for a hosted room here.** Open-and-close against a real room is
  `session_room_acceptance.rs` territory (node 1 / node 11); this node changes the type a closer is
  reached through, not what closing does.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-session-rooms-initial-discovery.md`)
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
