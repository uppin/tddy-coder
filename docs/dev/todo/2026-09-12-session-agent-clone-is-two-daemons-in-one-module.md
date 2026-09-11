# 2026-09-12 — `session_agent_clone.rs` is two daemons in one module, and it drags LiveKit into a crate that says it has none

**Category:** Future enhancement
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), changeset
[`2026-09-09-unbundle-session-agent-services`](../changesets/2026-09-09-unbundle-session-agent-services.md)

`packages/tddy-session-agents/src/session_agent_clone.rs` is 1,157 production lines and the module's
own header states the seam plainly: it holds **two sides of one feature**, and which side a type
belongs to is the first thing a reader has to establish.

| Side | Types | Runs on |
|---|---|---|
| facilitating | `SessionAgentCloneStore`, `AgentClone` | the daemon that owns the session and its roster |
| owning | `HostedAgentClones`, `HostedClone`, `run_clone_mirror` | the daemon that holds the checkout |

The facilitating half is bookkeeping: a map, a claim, a report, a teardown. The owning half is a
LiveKit participant — it joins `session-{session_id}`, runs the `tddy-session-sync` mirror
in-process, mints and re-mints admission tokens, and proxies mutating tool calls back over the room.

## Why it matters beyond the line count

The owning half is the **sole** reason `tddy-session-agents` depends on `tddy-daemon-livekit` and on
`livekit` itself. And that contradicts the crate's own stated shape:
`packages/tddy-session-agents/src/ports.rs` opens by saying every port exists because *"none of them
reaches for a peer, a room or a config, because none of them can from here"*. One module in the same
crate does all three.

The cost is paid by everything that links the crate, and the crate is on `tddy-daemon`'s path.

## What closing it would take

Split the module at the seam the header already draws, and put the owning half behind a
`CloneTransport` port in `ports.rs` — the same treatment `AgentConversationPeers` already gets for
the other forward this crate makes. The facilitating half then needs no room and no `livekit`, the
port's implementation lives in `tddy-daemon` beside the other transport wiring, and the crate's
dependency list matches what its own header claims.

Deferred rather than done at wrap because it is a real restructure with a behaviour surface (the
re-admit loop, the revocation signal, the mirror's local-changes gate), not a file move — and node 7
is a move-only node whose test coverage for that surface is `session_agent_remote_acceptance.rs`,
which needs two real daemons and Docker.
