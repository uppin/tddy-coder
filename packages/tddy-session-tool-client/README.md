# tddy-session-tool-client

How a session reaches its own daemon's `ExecuteTool` handler, over whichever transport that session
actually has. One selector reads the spawn environment, decides which of them is configured, and
dispatches the tool call.

## Quick Start

### Build
```bash
cargo build -p tddy-session-tool-client
cargo build -p tddy-session-tool-client --features livekit
```

### Test
```bash
cargo test -p tddy-session-tool-client
```

## Architecture

`SessionToolTransport` is the question "where is this session's daemon, and how do I reach it": the
in-jail unix socket, a direct HTTP Connect POST, or LiveKit RPC to a *remote* daemon holding a split
session's worktree. A fourth variant carries the absence of a usable one — some LiveKit variables set
and the rest not — because that is a different failure from "nothing configured" and falling through
to another transport would answer from the wrong host's filesystem. Detection and dispatch are one
place on purpose: the selector is the only thing that knows the whole set, and splitting it would
duplicate the decision. Every failure comes back as a `{"error": …, "is_error": true}` JSON string
rather than a typed error, because the caller hands it straight to a model as the tool's answer.

## Why it is its own crate

Every message it sends is a `tddy-service` proto, so `tddy-service` is where this belonged — and it
cannot go there. The LiveKit arm needs `tddy-livekit`, which has `tddy-service` in
`[dependencies]`, so the edge is `error: cyclic package dependency: package tddy-service depends on
itself`, and marking it `optional` behind a feature does not exempt it. Sitting **above** both
crates instead also keeps `tddy-service`'s dependency on `tddy-tui` out of the picture: a client
hosted there would have pulled the TUI into every in-jail binary that dispatches a tool call.

## The `livekit` feature is off by default

Unlike `tddy-tools`' own, and deliberately. `tddy-sandbox-app` and `tddy-sandbox-darwin` only ever
dispatch over the in-jail socket, and neither should link the LiveKit SDK to do it. Without the
feature `dispatch_via_livekit` still exists and reports the build-time omission, rather than
degrading to a transport aimed at the wrong host. The consumers that need a split session enable it
explicitly.

## Consumers

| Crate | How |
|---|---|
| `tddy-tools` | `[dependencies]`, re-exported as `tddy_tools::session_tool_client` — the path it was reached by before this crate existed. Its own default-on `livekit` forwards here |
| `tddy-discovery` | `[dependencies]` — how a jail reaches its facilitating daemon to follow the agent roster; its `livekit` feature forwards here too |
| `tddy-daemon` | `[dev-dependencies]`, with `livekit`. Its sandboxed-session suite drives dispatch end to end through env detection |
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
- [`tddy-livekit`](../tddy-livekit/) — the optional LiveKit transport
- [`tddy-sandbox`](../tddy-sandbox/) — `session_id_from_env`, the jail's own name for its session
- [`tddy-core`](../tddy-core/README.md) — `spawn_env::env_non_empty`, one spelling of "unset or blank"
