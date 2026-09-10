# 2026-09-10 — Log targets across the extracted crates still name `tddy_daemon`

**Category:** Future enhancement
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473)

Code that has left `tddy-daemon` still logs under the target it had there:

| Target | Now in |
|---|---|
| `tddy_daemon::auth`, `tddy_daemon::codex_oauth`, `tddy_daemon::github_token_store`, `tddy_daemon::oauth_tunnel` | `tddy-daemon-auth` |
| `tddy_daemon::common_room`, `tddy_daemon::livekit_peer_discovery::peer_metadata` | `tddy-daemon-livekit` |
| `tddy_daemon::host_private_key` and its siblings | `tddy-host-service` (node 1) |

**This is deliberate, not an oversight.** A log target is an operator's `RUST_LOG` filter, so
renaming these to match the crates would silently stop every filter already selecting them — and
the failure mode is *missing logs*, which is exactly what an operator turns to `RUST_LOG` to fix.
Every node of the stack has kept the precedent node 1 set.

The debt is that the targets now describe history rather than structure, and each node widens the
gap. A fleet-wide rename is one change, with a release note telling operators what to edit — not
something that should ride along with a subsystem move. Consider doing it once at the end of the
`#unbundle` stack, when the final crate layout is known and there is exactly one migration to
announce.
