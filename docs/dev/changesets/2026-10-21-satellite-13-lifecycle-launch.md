# 2026-10-21 — Lifecycle and launch off the LiveKit plugin; the ratchet closes

**Type:** Architecture

`#satellite` 13/13 ([#635](https://github.com/uppin/tddy-coder/pull/635)), the stack's closing node.
`tddy-session-lifecycle` and `tddy-agent-launch` name no LiveKit crate; the ratchet's
`KNOWN_OFFENDERS` is deleted, and no library crate outside the plugins names LiveKit.

## What changed

- **`SessionReach::open_now` and `OpenedRoom`.** The port opens a session's room now for a caller that
  cannot proceed without it (`Ok(None)`: no integration). The host's `ensure_session_room` (the
  agent-roster paths) and the plugin's `OpenSessionRoom` both call it; `LiveKitSessionReach`'s inherent
  `open_now` folded into the trait's. `require_opened_room` is the agent-roster check over it.
  `OpenedRoom` replaces `OpenedSessionRoom`.
- **The host on ports.** `DaemonSessionHost::new` takes a `PeerRouting` (`PeerRouting::local_only`,
  `tddy_daemon_livekit::peer_routing::peer_routing_over`) and installs each port's null object;
  `with_session_rooms`, `with_room_broadcast`, `with_room_admission`, `with_room_directory`,
  `with_room_tokens`, `with_room_connector` and `with_session_delta_stores` replace them.
  `tddy_daemon::livekit_host_ports::with_livekit_room_ports` wires the plugin's, for the runtime and for
  the `tddy-daemon` suites. `NoSessionDeltaStores` sits beside its port in `tddy-session-activity`.
  [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#wiring-the-session-host).
- **Moves.** `RoomsHostedByThisDaemon` → `tddy-daemon/src/session_delta_stores.rs` (not the plugin:
  that needs a `tddy-daemon-livekit` → `tddy-session-activity` edge, see
  [the backlog entry](../todo/2026-10-21-session-delta-stores-adapter-lives-in-the-daemon.md)).
  `session_type_is_facilitated_here` → `tddy_agent_launch::facilitation`; `snapshot_worktree*` and the
  deadline-bounded git helpers → `tddy_worktree_service::worktree_measurement`;
  `classify_start_session_peer_route` / `StartSessionPeerRoute` removed for the kernel's
  `classify_peer_route` / `PeerRoute`; lifecycle's seven plugin re-exports removed;
  `serve_daemon_rpc_participant` → `tddy-daemon/tests/daemon_rpc_participant/mod.rs` (lifecycle keeps
  `test_util::daemon_rpc_roster`) rather than `tddy-livekit-testkit`, which cannot depend on the host.
- **Tests.** `tddy-session-lifecycle/tests/host_without_integrations.rs` (nothing wired: start, local
  connect, resume, delete); `tddy-session-agents/tests/opened_room.rs`; the delta adapter's unit tests
  (each scope, unknown call, aged out); `only_plugins_and_wiring_name_a_livekit_crate`. About 85
  fixtures moved to `PeerRouting::local_only`.

## Code issues

- `oversized-file-session-coordinate-handlers` (agent-launch): 503 → 522 by this PR, then the
  `facilitation` module split it to 484. **Closed**, record deleted.
- `oversized-file-session-room`: 2,857 → 2,652. `oversized-file-livekit-peer-discovery`: 1,407 → 1,390.
  `oversized-file-connection-service`: −29. Narrowed, still open.
- `oversized-file-runtime`: 1,858 → 1,867; `complexity-runtime-build`: 1,071 → 1,080. Regressed,
  recorded for the stack's sweep node.

## Not changed

Every behaviour of nodes 1–12. The `tddy-daemon` binary still always wires LiveKit
([backlog](../todo/2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin.md)).
