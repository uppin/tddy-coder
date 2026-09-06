# 2026-09-06 — Starting a session no longer waits on LiveKit

Creating a session is local work again. A worktree, a branch, a session directory and an agent
process — nothing in that sequence contacts LiveKit, so an operator whose LiveKit server is
configured but unreachable can start sessions exactly as one who never configured it can. Before,
such an operator could not start a session **at all**: the daemon opened the session's LiveKit room
during the start, waited on a server that was not answering, and the browser eventually reported its
own request as timed out.

A session's room `session-{session_id}` is now created by the first thing that needs to reach that
session over LiveKit — a client connecting to it, a `tddy-session-sync` mirror, a remote agent being
attached — rather than at the moment the session is created. The same connection puts the
facilitating daemon in the room *and* tells the caller which room to look in, so the daemon is still
that room's first participant; nothing that had a room has lost one, it simply appears when it is
first wanted. A session created while the server was down becomes reachable over LiveKit the moment
the server is, with no restart.

The terminal a remote client drives moves with it. A `claude-cli` session's PTY is bridged into the
common room by the same connection, under the same interlock as the room, so a session can never be
found by a remote client with a terminal it cannot type at. The desktop reaches its own host over IPC
and drives that terminal without a bridge at all, so a desktop-only deployment now creates neither.
A session started from Telegram still bridges immediately, because its reply hands a human the room
and identity to attach with — there the consumer has already arrived.

**Failures did not go quiet.** A connection that needs the room and cannot create it is refused, and
told which room could not be created. What is unblocked is session creation, not LiveKit error
reporting. Two clients connecting at once still get one room and one terminal bridge between them.

A split-placement session is the one start that still opens its room itself: LiveKit is that
placement's transport to its own checkout, and the agent it spawns is handed a token minted for that
room before anything could have connected.

See [session-room.md](../session-room.md) and
[session-worktree-sync.md](../session-worktree-sync.md).
