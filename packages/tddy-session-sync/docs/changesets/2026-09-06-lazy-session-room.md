# 2026-09-06 — A mirror asks for the room before joining it

**Type:** Fix

Node 9 of the `optional-livekit` stack ([#449](https://github.com/uppin/tddy-coder/pull/449) is its
parent). See the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-06-lazy-session-room.md](../../../../docs/dev/changesets/2026-09-06-lazy-session-room.md).

A session's LiveKit room is now created by the first thing that connects to the session over LiveKit
rather than by the session's own start, and a mirror is such a thing. `attach` therefore calls
**`ConnectSession`** between resolving the session over HTTP and minting its room token.

The reply is deliberately unread: its LiveKit fields name the session's *terminal* room, while a
mirror joins `session-{session_id}`, which it derives itself with `session_room_name`. What is wanted
is the call's effect — `daemon-{instance_id}` in the room before anything looks for it there.

Without it a mirror would join a room LiveKit auto-creates on its behalf, containing no facilitating
daemon, which is byte-for-byte the same experience as the existing `DaemonAbsent` wait expiring: a
mirror that never syncs and a failure that names the wrong cause.

Module [mirroring.md](../mirroring.md); feature
[session-worktree-sync.md](../../../../docs/ft/daemon/session-worktree-sync.md),
[session-room.md](../../../../docs/ft/daemon/session-room.md).
