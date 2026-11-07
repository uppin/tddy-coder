# 2026-11-07 — A dropped RPC call leaves its pending entry in the client engine

**Category:** Deferred from `#agent-tools` 23/28 (`feature/agent-tools/bounded-waits`)
**Source:** `packages/tddy-rpc/src/client_engine.rs:70`, `:105-123`; found while bounding the supervisor client's calls in `packages/tddy-supervisor/src/client.rs` (changeset `docs/dev/1-WIP/2026-11-06-bounded-session-waits.md`)

## What is there

`ClientEngine` keeps every in-flight call in `pending_unary` (and `pending_streams` for server streams),
keyed by request id. `register_unary` inserts the entry, and the entry leaves only when a reply
arrives for its id or the connection closes.

Dropping the future of a `call_unary` does not remove the entry. The same holds for a dropped
`call_server_stream` receiver. Once the supervisor client wraps its calls in a deadline, every
timed-out call leaves an entry behind.

## Why it matters

- **Memory and bookkeeping.** Each timed-out call keeps a `oneshot` sender and its call descriptor
  alive for as long as the connection stays open.
- **The side effect is unknown to the caller.** A timed-out request may already have been acted on by
  the supervisor, for example a spawn or a scope. The daemon is never told, because the reply is
  dropped on arrival.

## Why it was deferred

The engine is shared by every RPC client in the workspace, so a change to its cancellation semantics
reaches all of them. It is not part of the deadline work, which only needs each call to fail in time.
The bounded-waits changeset ships the deadlines and records this leak as debt.

## What a fix needs

- On drop of the call future or stream receiver, remove its pending entry (a drop guard keyed by id),
  so the map cannot grow with abandoned calls.
- Decide what the daemon does about a request the supervisor may already have acted on. Removing the
  entry alone does not answer that. It needs a protocol-level answer, such as an idempotency key or a
  reconcile on the next status read.
