# Initial discovery: session creation must not block on LiveKit

**Stack:** `optional-livekit` — node 9 of 9. **Date:** 2026-09-06.
**Source:** a reproduced production failure, not a hypothesis.

## The failure

An operator ran the desktop app with LiveKit **configured but unreachable**
(`ws://192.168.1.10:7880`) and created a Claude Code CLI session. It never started. Browser console:

```
[internal] creating session room session-01a0758f-…: creating LiveKit room session-01a0758f-…
Failed to load resource: The request timed out. (validate)
```

The daemon opened a per-session LiveKit room during session start, blocked on the unreachable
server, and failed the whole operation. The session directory was left holding a `changeset.yaml` at
`state: Init`, with a worktree and a pushed branch but no agent.

## The asymmetry

`SessionRoomRegistry::open_measured_by` (`session_room.rs:1446`) degrades gracefully when LiveKit is
**absent**:

```rust
let Some(credentials) = LiveKitCredentials::from_config(hosting.config) else {
    log::debug!("session_room: no LiveKit credentials configured; session {} keeps its worktree and hosts no room", …);
    return Ok(None);
};
```

There is no equivalent path for **configured-and-unreachable**. `create_room`
(`session_room.rs:1757`) turns the failure into `Status::internal`, which kills session start. So the
deployment that never had LiveKit is fine and the deployment whose LiveKit is merely *down* is not —
which is backwards.

**The variable is reachability, not session type.** An earlier attempt at this node tried to gate room
creation on session type and was stopped before any code was written, because that framing is wrong:
see below.

## The trap: there are TWO rooms, and the obvious comment is about the other one

`ConnectSession` (`connection_service.rs:12617`) reads:

```rust
// claude-cli, cursor-cli, and workspace sessions do not use LiveKit — return empty fields immediately.
```

**That comment is imprecise and it is a trap.** It is about the session's *terminal room*
(`metadata.livekit_room`), which those three types write as `None` at spawn
(`connection_service.rs:3778`, `:6516`, `cursor_cli_spawn.rs:420`, `workspace_session.rs:151`). It is
**not** a statement that those types do not use LiveKit.

The room failing in production is the other one — the **session room** `session-{id}`. Its contract,
`docs/ft/daemon/session-room.md`, opens:

> Every session that runs an agent has its own LiveKit room, `session-{session_id}`, hosted by that
> session's **facilitating daemon** … A session with no agent — a standalone `workspace` session —
> has no facilitating daemon and no room.

`claude-cli` and `cursor-cli` are precisely the types that run agents. `claude-cli` in fact uses
LiveKit twice: the session room, plus a **PTY bridge** into the common room
(`connection_service.rs:3805-3855`). Gating on session type would have deleted a documented feature.

**Reword that comment as part of this node.** It is the sentence that produced the wrong design once
already.

## No timeout anywhere

`packages/tddy-livekit/src/room_metadata.rs` contains **zero** timeout handling, and neither does
`create_room`'s call into it. That is why the operator saw the *browser's* `/rpc` request time out
rather than a daemon error: nothing bounded the control-plane call. Independent of any other fix, a
bounded call turns a hang into a fast, legible failure.

## Who actually needs the session room

Deferring creation is only safe if every consumer can trigger it. Beyond `ConnectSession`:

| Consumer | Where |
|---|---|
| Split-placement start (claude-cli only) | `connection_service.rs:9854` |
| Remote agent attaching to a `workspace` session | `connection_service.rs:4314` `ensure_session_room_for_agents` — returns `failed_precondition`, deliberately, when it cannot |
| `tddy-session-sync` worktree mirroring | `packages/tddy-session-sync/src/attach.rs:173,255` |
| Seeded agent clones | `session_agent_clone.rs:484` |
| Participant admission | `session_admission_service.rs:196` |
| Split agent's `TDDY_REMOTE_SERVER_IDENTITY` | `split_session.rs:589` |

`open_session_room_before_spawning_agent` (`connection_service.rs:3192`) is the eager opener; all four
of its call sites pass `claude-cli` or `cursor-cli` (`:3734`, `:6424`, `:6900`,
`cursor_cli_spawn.rs:387`).

## Precedent for tolerating a LiveKit failure at spawn

The PTY bridge in the **same** session start already does it
(`connection_service.rs:3843-3852`): on failure it logs `warn!` — "LiveKit bridge failed; gRPC path
still works" — and carries on. So session start is already half-tolerant of LiveKit being down; the
two halves simply disagree. Making them agree is the shape of this node.

## Adjacent, and deliberately not this node

- **`connection_service.rs:10713`** — `livekit_creds_from_config(...).ok_or_else("LiveKit not
  configured")`. Narrower than it first looks: the claude-cli and cursor-cli branches return before it
  (`:10611`, `:10646`, `:10675`), so it gates only the remaining `tool` session path. Still a real
  defect.
- **`session_deletion.rs:220`** — the comment "A session that never hosted a room (any type but
  `workspace`…)" is backwards on both halves. Stale comment, no behaviour attached.
