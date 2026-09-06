# 2026-09-06 — A session starts without waiting on LiveKit

**Type:** Fix

Node 9 of the `optional-livekit` stack, sitting above node 8
([#449](https://github.com/uppin/tddy-coder/pull/449)) but depending on nothing it delivered — the
stack is linear and both are daemon-side, so keeping them adjacent minimised the conflict surface.

An operator with LiveKit **configured but unreachable** could not start a session at all. Not a
degraded session, not one without media — none. The daemon opened the session's room
`session-{session_id}` before spawning the agent, that call had no timeout at any layer, and the
failure became `internal`, killing the start; the browser reported its own `/rpc` request as timed
out and the session directory was left at `state: Init` with a worktree, a pushed branch and no
agent. Reproduced in production, not hypothesised. The deployment that never configured LiveKit was
fine, because that path returned `Ok(None)` — so the failure landed on exactly the operator who had
configured it and whose server was merely down. This was the last place the stack's premise was
untrue: nodes 1–7 made the client work without LiveKit, node 8 made it switchable off, and a session
still could not be *created* without it answering.

**Session creation is local work, and now stays local.** A worktree, a branch, a session directory,
an agent process. For the ordinary placement, where the agent and its checkout are on one daemon, a
start contacts LiveKit nowhere — configured or not, reachable or not. That is the property worth
keeping: a future change must not silently put a LiveKit call back on that path. A session created
while the server was down becomes reachable over LiveKit the moment the server is, without a restart.

A session's room is created by the first thing that needs to reach it over LiveKit: a client
connecting, a `tddy-session-sync` mirror, a `tddy-tools pty-relay --livekit-url`, a remote agent
being attached, a seeded agent's clone being claimed. The invariant that mattered survives —
`ConnectSession` is both what puts `daemon-{instance_id}` in the room and what tells the caller which
room to look in, so the facilitating daemon still cannot be beaten to it. Everything that had a room
has one, at the moment it is first needed.

**Lazy, not conditional, and the difference is the whole design.** The alternative — deciding up
front which sessions need a room — was attempted and stopped before any code was written. The only
signal available that early is the session type, and the types that look exempt, `claude-cli` and
`cursor-cli`, are exactly the ones that run agents and therefore *do* have rooms.
`ConnectSession`'s early return for those types is about a session's **terminal** room, a different
room with different participants; reading it as "these types do not use LiveKit" is what produced the
wrong design, and the distinction is now recorded in the docs and in the predicate that names it so
it cannot produce a second.

Concurrency is handled by a per-session lock held across the create-and-join, because the registry
offers no idempotency of its own: asking whether a room is open and then opening it are two steps,
registering replaces the entry and aborts the previous room's tasks, and a second join under one
identity has the server evict the first. Two connects at once therefore yield one room and one
terminal bridge, not two of each.

**The terminal bridge moved with the room.** A `claude-cli` session's PTY is served to LiveKit
clients by a participant in the common room, which is what lets a client that is *not* on this host
drive the terminal — LiveKit work, so it belongs where LiveKit work now happens. It is established by
the same connect under the same lock, so a session cannot be findable by a remote client with a
terminal it cannot type at. The desktop reaches its own host over IPC and needs no bridge at all, so
a desktop-only deployment creates neither, ever. Where the terminal will be served is a pure function
of the config and the session id, so `StartSession` reports those coordinates without contacting
anything: deferring *when* the participant joins does not move *where* it is.

Two starts are deliberately not lazy, and both because their consumer has already arrived. A
**split-placement** start opens the room itself — LiveKit is that placement's transport to its own
checkout, and the agent it spawns is handed a token minted for that room before anything could have
connected. A **Telegram** start bridges its terminal immediately, because its reply hands a human the
room and identity to attach with; deferring would make that message an invitation to an empty room.

**Failures stayed loud.** A connection that needs the room and cannot create it is refused, naming
the room. Making room failures non-fatal at the point of use was explicitly out of bounds: it would
cost a session its sync, clone and split capability with a log line to show for it. What is unblocked
is session creation.

`tddy-session-sync` and `tddy-tools pty-relay --livekit-url` each gained a `ConnectSession` call
before joining. Both then wait for `daemon-{instance_id}` to be in the room, and joining without
asking would join a room LiveKit auto-creates on their behalf with no daemon in it —
indistinguishable from that wait timing out.

⚠️ Known and unchanged: the LiveKit room-creation call still has no timeout, so a server that accepts
a connection and never answers makes the *connect* wait rather than fail fast. It is off the start
path, which is what the bug was about, and recorded in `docs/dev/TODO.md`. A split session's room is
still not re-opened after a daemon restart, for the same reason it is opened eagerly; a co-located
session now recovers by itself on the next connect.

Feature docs [session-room.md](../../ft/daemon/session-room.md),
[session-worktree-sync.md](../../ft/daemon/session-worktree-sync.md); technical
[session-room.md](../../../packages/tddy-daemon/docs/session-room.md),
[mirroring.md](../../../packages/tddy-session-sync/docs/mirroring.md).
(tddy-daemon, tddy-session-sync, tddy-tools)
