# 2026-10-17 — LiveKit as a satellite: extension ports, local ConnectSession, status board, ratchet

**Type:** Architecture

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)). A local session attach no longer depends on LiveKit: a Tddy Desktop whose
`livekit:` block pointed at a server that did not answer started its sessions but could not attach
their terminals, because `ConnectSession` opened the session room before it answered and the open
failed. The room now opens through a core port, so the core depends on a port rather than on LiveKit.

## What changed

- **`tddy-daemon-extensions` (new).** `SessionReach`, `ReachableSessions`, `NoSessionReach`, and the
  integration status board. Nothing in it reaches a LiveKit crate.
  [Extension ports](../../../packages/tddy-daemon-extensions/docs/extension-ports.md).
- **`ConnectSession`** reports a reach through `SessionReach` with the host-stamped request transport
  and never awaits LiveKit for a caller that did not arrive over it
  ([`tddy-agent-launch`](../../../packages/tddy-agent-launch/docs/module-layout.md#connectsession-and-the-session-reach-port),
  [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/docs/session-service.md)).
- **`tddy-daemon-livekit`** implements the port: synchronous open for a caller that arrived over
  LiveKit, background open for everyone else, and `SessionRoomService.OpenSessionRoom`, which stays
  loud and refuses a session this daemon does not facilitate
  ([LiveKit session reach](../../../packages/tddy-daemon-livekit/docs/livekit-service.md#livekitsessionreach--opening-the-room-for-a-reached-session)).
- **Integration status.** `DaemonConfigService.GetIntegrationStatus` reads the board; LiveKit reports
  from every room open and from the common-room discovery loop
  ([daemon endpoint](../../../packages/tddy-daemon/docs/daemon-endpoint.md#integration-status-and-the-livekit-plugin),
  [protos](../../../packages/tddy-service/docs/integration-status-proto.md)).
- **Clients.** `tddy-session-sync` and `tddy-terminal-rpc`'s `pty_relay` ask `OpenSessionRoom`
  ([mirroring](../../../packages/tddy-session-sync/docs/mirroring.md#opening-the-room-is-asked-for-not-assumed),
  [terminal service](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md#opening-the-sessions-room-for-the-livekit-relay)).
- **`WorkspaceGraph`** in `tddy-testing-commons`, and the ratchet
  `tddy-daemon-extensions/tests/livekit_is_a_satellite.rs`
  ([graph](../../../packages/tddy-testing-commons/docs/dependency-graph.md),
  [testing guide](../guides/testing.md#dependency-boundary-tests)).
- **Product behaviour:** [session rooms](../../ft/daemon/session-room.md),
  [daemon settings](../../ft/daemon/daemon-settings.md#integration-status),
  [session worktree sync](../../ft/daemon/session-worktree-sync.md),
  [terminal sessions](../../ft/daemon/terminal-sessions.md).

## Measurements

- **Known offenders at this node: 18** library crates still name a LiveKit crate, listed in the
  ratchet's `KNOWN_OFFENDERS`: `tddy-agent-launch`, `tddy-cli-sessions`, `tddy-coder`,
  `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-rpc`, `tddy-host-service`,
  `tddy-remote-git-repo`, `tddy-screen-sharing`, `tddy-session-activity`, `tddy-session-agents`,
  `tddy-session-files`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-session-sync`,
  `tddy-session-tool-client`, `tddy-telegram-control`, `tddy-terminal-rpc`. Beside them the allowed
  set is 5 plugin crates and 4 wiring or harness crates. Computed from `cargo metadata`
  2026-10-16. **The list only shrinks**: a crate that stops naming a LiveKit crate must be struck
  from it, and a crate that starts and is on no list fails the build.
- Production lines of the files this change grew, all already over the 500-line budget and all
  deferred with the developer's consent: `session_room.rs` 2,874 -> 2,935, `runtime.rs` 1,781 ->
  1,811, `attach.rs` 520 -> 536, `pty_relay.rs` 774 -> 776, `livekit_peer_discovery.rs` 1,658 -> 1,697 and `tddy-service/build.rs` 723 -> 737; each is
  recorded in its package's `docs/code-issues/`.

## Backlog

Resolved and removed: **2026-09-10 — The dependency-boundary harness is duplicated in every extracted
crate** (`WorkspaceGraph` replaces both copies, covers table-form and workspace-inherited
dependencies with named tests, and the "reaches the kernel" guard is kept).

Narrowed, kept: **2026-09-06 — From the 2026-09-06 `#optional-livekit` stack evaluation**. The status
board now exists and LiveKit reports into it; moving the remaining re-derivations of "is LiveKit
configured?" onto it is the rest of the `#satellite` stack.

## Known gaps

- `join_room` has no deadline: a background open against a hanging server holds that session's
  opening lock, and an `OpenSessionRoom` for the same session waits behind it.
- `ensure_session_room` and the agent-roster paths still open rooms directly; later nodes of the
  stack move them behind ports.
- A session-room open and the discovery loop write one status entry, last writer wins.
- The boundary suites no longer see `[dev-dependencies]`, so their "does not declare the daemon" tests
  are weaker by that edge.
