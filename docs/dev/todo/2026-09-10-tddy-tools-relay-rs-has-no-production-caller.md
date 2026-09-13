# 2026-09-10 — `tddy_tools::relay` has no production caller

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M8

`packages/tddy-tools/src/relay.rs` — 135 raw lines, 119 non-blank — finds or spawns a **`tddy-daemon` relay
process**: a TCP probe on the configured port, a `daemon.json` discovery file, a `--relay` spawn if
neither answers. Its one entry point is `ensure_relay_daemon(&RelayConfig) -> Result<RelayEndpoint>`.

**Nothing in any crate's `src` calls it.** The only reader anywhere in the workspace is its own
`packages/tddy-tools/tests/relay_ensure_acceptance.rs`, which is therefore a suite that tests a
function no product path reaches.

The node-5 plan put it in `tddy-discovery`, and the move was **not** taken (see the changeset's
`## Boundaries`): the match was a name collision rather than a domain one — `tddy-discovery` is the
*codebase-exploration* agent, and this module is daemon process management. Moving it would also
have had to add `anyhow` to `tddy-discovery`, which has none, for a destination that was wrong to
begin with.

So it needs a decision the restructure could not make for it, and both answers are cheap:

- **Retire it**, with `relay_ensure_acceptance.rs`, if spawning a relay daemon from a tool process
  is no longer how anything reaches a daemon — which the four transports in
  `tddy_session_tool_client::detect_session_tool_transport` suggest.
- Or **give it a real home** next to whatever is supposed to call it (`tddy-daemon-kernel`'s
  process/config surface is the closest fit) and wire that caller.

Deferred because either answer changes behaviour or deletes a suite, and node 5 is a
behaviour-preserving restructure that asserts nothing changed.
