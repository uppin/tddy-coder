# tddy-session-tool-client

How a session reaches its own daemon's `ExecuteTool` handler, over whichever transport that session
actually has. One selector reads the spawn environment, decides which of them is configured, and
dispatches the tool call.

## Quick Start

### Build
```bash
cargo build -p tddy-session-tool-client
```

### Test
```bash
cargo test -p tddy-session-tool-client
```

## Architecture

`SessionToolTransport` is the question "where is this session's daemon, and how do I reach it": the
in-jail unix socket, a direct HTTP Connect POST, or room RPC (LiveKit, in practice) to a *remote*
daemon holding a split session's worktree. A fourth variant carries the absence of a usable one — some LiveKit variables set
and the rest not — because that is a different failure from "nothing configured" and falling through
to another transport would answer from the wrong host's filesystem. Detection and dispatch are one
place on purpose: the selector is the only thing that knows the whole set, and splitting it would
duplicate the decision. Every failure comes back as a `{"error": …, "is_error": true}` JSON string
rather than a typed error, because the caller hands it straight to a model as the tool's answer.

## Conversations

`dispatch_conversation_tool(conversation, tool, args)` is `dispatch_session_tool` with the
conversation named on the wire, so the daemon runs the call in that conversation's worktree;
`conversation_worktree(conversation, op)` asks for `ConversationWorktreeOp::{Pull, Remove}` over the
same four transports; `reset_conversation_worktree(conversation, commit)` sends the `Reset` op (an
empty commit names the conversation's base) and `diff_conversation_worktree(conversation, from, to)`
sends the `Diff` op (an omitted bound is sent empty: the base for `from`, the tip for `to`);
`pull_conversation_range(conversation, from, to, already_pulled)` sends `PullRange`; and
`sync_conversation_worktree(conversation)` sends `Sync`, which merges the session worktree's current
files into the conversation's worktree before a turn and answers `{"sync": {…} | null}` or
`{"conflicts": […], "moreConflicts": n}`. They are functions beside `conversation_worktree` rather than
`ConversationWorktreeOp` variants, because that enum is `Copy` and they carry data; each builds its
request through `conversation_op_request` from an `Op`. All of them live in `src/conversation.rs`,
and the transports' request-taking cores (`dispatch_request_via_*`) are shared with the
`dispatch_via_*` entry points.

Over HTTP the request travels as Connect JSON built by `connect_json_body`: the request's fields, and
the chosen operation as a key of its own — `{"pull": {}}`, `{"remove": {}}`, `{"reset": {"commit"}}`,
`{"diff": {"from", "to"}}`, `{"pull_range": {"from", "to", "already_pulled"}}`, `{"sync": {}}`. That
encoding is hand-written and not yet verified against a live daemon over HTTP; see the backlog entry
*The `ConversationWorktree` call over HTTP is unverified*.

## Why it is its own crate

Every message it sends is a `tddy-service` proto, so `tddy-service` is where this belonged — and it
cannot go there. A LiveKit arm would need `tddy-livekit`, which has `tddy-service` in
`[dependencies]`, so the edge is `error: cyclic package dependency: package tddy-service depends on
itself`, and marking it `optional` behind a feature does not exempt it. Sitting **above**
`tddy-service` also keeps its dependency on `tddy-tui` out of the picture: a client hosted there would
have pulled the TUI into every in-jail binary that dispatches a tool call.

## The room transport is installed, not linked

The crate names no LiveKit crate. The remote-room arm joins through
[`tddy_room_client::RoomConnector`](../tddy-room-client/docs/room-client.md), which the binary installs
once at startup with `install_room_connector` (a second call fails with
`RoomConnectorAlreadyInstalled`). The connector is process-wide because `dispatch_session_tool`, the
conversation calls and the roster's `livekit_session` are called from too many places for a
parameter. Until one is installed, a split session's dispatch fails with the message of
`NoRoomTransport` rather than reaching a room, and never degrades to a transport aimed at the wrong
host. `room_session_via(cache, key, connector)` takes the connector as a parameter and is what the
tests drive; the room is joined once per destination and the session is reused from the cache
(`tests/room_transport_cache.rs`). `tddy-sandbox-app` and `tddy-sandbox-darwin` link no LiveKit SDK to
use this crate.

## Consumers

| Crate | How |
|---|---|
| `tddy-tools` | `[dependencies]`, re-exported as `tddy_tools::session_tool_client` — the path it was reached by before this crate existed. It installs the room connector (LiveKit's, behind its default-on `livekit` feature) |
| `tddy-discovery` | `[dependencies]` — how a jail reaches its facilitating daemon to follow the agent roster, over the same installed room transport |
| `tddy-daemon` | `[dev-dependencies]`. Its sandboxed-session suite drives dispatch end to end through env detection |
| `tddy-sandbox-app`, `tddy-sandbox-darwin` | `[dev-dependencies]`, default features. Both dropped their `tddy-tools` dev-dependency when this crate appeared |

## Documentation

### Technical implementation (how)
- [Changesets](./docs/changesets/) — applied changeset history
- [`tddy-tools`](../tddy-tools/README.md) — the binary that advertises the tools this dispatches

### Product requirements (what)
- [Sandboxed codebase mode](../../docs/ft/coder/sandboxed-codebase-mode.md) — the jail whose socket is the first transport
- [Remote managed worktree](../../docs/ft/daemon/remote-managed-worktree.md) — the split session the LiveKit transport exists for

## Related packages
- [`tddy-service`](../tddy-service/) — every proto this crate sends
- [`tddy-room-client`](../tddy-room-client/docs/room-client.md) — the room transport port this crate joins through
- [`tddy-sandbox`](../tddy-sandbox/) — `session_id_from_env`, the jail's own name for its session
- [`tddy-agent-backend`](../tddy-agent-backend/README.md) — `spawn_env::env_non_empty`, one spelling of "unset or blank"
