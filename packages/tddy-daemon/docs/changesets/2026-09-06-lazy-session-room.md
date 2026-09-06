# 2026-09-06 — A session's room is opened by the first connection to it

**Type:** Fix

Node 9 of the `optional-livekit` stack, on top of node 8
([#449](https://github.com/uppin/tddy-coder/pull/449)) but independent of it. See the cross-package
entry for the whole change:
[docs/dev/changesets/2026-09-06-lazy-session-room.md](../../../../docs/dev/changesets/2026-09-06-lazy-session-room.md).

`SessionRoomRegistry::ensure_open` replaces the eager open on every agent-start path.
`SessionRoomHost` and `open_session_room_before_spawning_agent` are gone, along with the `room_host`
parameter threaded through `spawn_claude_cli_session_inner`, `spawn_cursor_cli_session_inner`, both
sandboxed spawns and the two child-spawn handlers; `DaemonSessionRoomHost` survives as
`DaemonSeedCloneClaimant`, which is the only capability those call sites still needed.
`ConnectionServiceImpl::ensure_session_room` is the single daemon-side entry point, called from
`ConnectSession` and from `ensure_session_room_for_agents`. A split start still calls
`open_measured_by` directly: its agent is handed a token minted for the room before anything could
have connected.

**`ensure_open` is single-flighted, not merely idempotent.** The registry offers no idempotency —
`hosts` then `open` is two steps, `register` replaces the entry and aborts the previous room's tasks,
and a second join under `daemon-{instance_id}` has LiveKit disconnect the participant already there —
so a per-session `tokio::sync::Mutex` is held across the create-and-join. `already_open` derives the
reusing caller's answer from the session id, the instance id and the config rather than keeping a
second copy of three values that cannot disagree, and `close` drops the lock entry so the map holds
one per connected session rather than one per session ever seen.

The PTY bridge became lazy under the same lock. `CliSessionManager` gained a `livekit_terminals`
registry: `expose_terminal_to_livekit` records at spawn what a session publishes about itself —
including the stack association nothing on disk records — and `ensure_livekit_terminal` puts the
participant in the room when a consumer first arrives, returning whether the terminal is drivable.
`false` is not a failure (no such session type, or an agent that has exited leaving no PTY); a
credentialed failure is, and surfaces as `internal` naming the room and identity.
`spawn_livekit_bridge` now returns its `JoinHandle` so a finished task reads as a bridge that is no
longer there, and terminal cleanup forgets the entry with the main terminal.

`ConnectSession`'s comment about `claude-cli`, `cursor-cli` and `workspace` "not using LiveKit" was
reworded and split: it was always about the *terminal* room, those types run agents and do have
session rooms, and that imprecision had already produced one wrong design. The predicate that names
the distinction lives in `session_room::session_type_is_facilitated_here`.

**Four tests were removed on purpose.** `cursor_cli_start_fails_when_the_session_room_cannot_be_opened`,
`cursor_cli_agent_is_not_spawned_when_the_session_room_cannot_be_opened`,
`cursor_cli_start_hosts_the_room_of_the_session_it_starts` and
`the_facilitating_daemon_is_the_only_participant_when_start_session_returns` all pinned "a session
whose room cannot be opened must not start", which is the rule this change reverses. What replaces
them, in `tests/session_room_acceptance.rs`, asserts the absence of the call rather than a successful
return: `starting_a_session_dials_livekit_not_at_all`,
`a_session_gets_its_room_when_something_first_connects_to_it`,
`connecting_again_goes_on_using_the_room_the_first_connect_opened`,
`connecting_to_a_session_fails_when_livekit_cannot_be_reached`, and the terminal-bridge siblings of
each.

⚠️ Known: the LiveKit room-creation call still has no timeout, so an unreachable-but-accepting server
makes the *connect* wait rather than fail fast. Out of the start path, which is what mattered;
recorded in `docs/dev/TODO.md`.

Module [session-room.md](../session-room.md); feature
[session-room.md](../../../../docs/ft/daemon/session-room.md).
