# 2026-09-05 — Many concurrent addressed connections over the desktop IPC bridge

**Type:** Architecture

Node 6 of the `optional-livekit` stack ([#442](https://github.com/uppin/tddy-coder/pull/442)). Nodes
1–5 made `tddy-web`'s daemon and session model transport-neutral; this node supplies the transport
that model needs on the desktop, where until now there was exactly one connection and it reached
exactly one thing.

The bridge was single-connection at all three layers by construction: `tddy-tauri-rpc`'s host held
one `Option<Connection>` and `connect` abandoned whatever was in it, `ipc.rs` exposed two commands
with no address on either, and `daemonTransport.ts` kept a module-level bridge singleton precisely
*because* registering a second response channel abandoned the page's own first connection. So a page
got one connection, that connection reached the daemon's roster, and session-scoped work on the
desktop fell back to the daemon.

Four decisions are worth keeping outside the desktop's own docs:

- **The client epoch was already the connection id.** It is minted per connection on the page side
  and stamped on every frame, so keying the host's connection map by it made multi-connection a
  change of container rather than of protocol: no frame format change, no version to negotiate, and
  the engine's peer naming (`webview-{epoch}`), its per-connection bounded queue and its
  release-a-peer's-state-as-a-unit behaviour all carried over untouched.
- **A separate host, rather than a wider one.** `MultiConnectionHost` is a new type;
  `WebviewRpcHost` is unchanged and still exported for a host that genuinely serves one webview. The
  crate's tests run the shared call-shape and lifecycle bodies against both, so the single-connection
  path is pinned rather than left to rot, and the two part only where they should — on what
  *opening* a connection does to one already open.
- **The resolver, not the host, knows what a target means.** `tddy-tauri-rpc` is handed a
  `RosterResolver` and stays a generic webview-RPC host; only the desktop knows how its own
  addressing works. That is also where the honest caveat lives: the crate refuses an unresolvable
  target at connect, with a reason, but the desktop's resolver resolves *every* target, so that
  refusal never fires there. It is not gated on a live session because resolution is synchronous and
  the lookup that could answer is async — widening the trait for one host's convenience is the wrong
  trade, and the daemon answers a call naming an unknown session with its own error anyway.
- **The target is a closed enum, deliberately.** An open string target would let the LiveKit identity
  strings this stack exists to remove (`daemon-{instance}-{session}`) leak across the IPC boundary,
  and once one had, nothing would notice. A new kind of peer should be a new variant.

**A connection now has a lifetime of its own, and that is what makes the rest necessary.** Sessions
come and go far more often than pages do, so `tddy_rpc_disconnect` exists or every attach leaks a
host-side peer. A page reload used to reap the previous page's connection for free, because one slot
was overwritten; with a map it is explicit, awaited at the commit of the arriving document, before
that document's scripts can open connections the reap would take with them.

One change outside the packages this node owns: `tddy-rpc` gained a delegating
`impl<S: RpcService + ?Sized> RpcService for Arc<S>`, so a roster handed back as `Arc<dyn RpcService>`
can drive a `ServerEngine`. Additive, no new crate in the tree, and consistent with `ServiceEntry`,
which already stored rosters this way.

**What is deferred on purpose.** This node ships the transport, not a provider. The desktop
application does not yet open session connections — the dashboard still reaches everything over its
daemon connection — and registering a connection provider over this transport is node 7
([#443](https://github.com/uppin/tddy-coder/pull/443)).

No proto change, no daemon change, no new npm dependency. `serde` is now a direct dependency of
`tddy-desktop` (and `serde_json` a dev-dependency) because a Tauri command argument must be
`Deserialize`; both were already linked through `tauri`, so no crate enters the tree.

Product [tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md), technical
[webview-ipc-connections.md](../../../packages/tddy-desktop/docs/webview-ipc-connections.md), package
changeset
[2026-09-05-multi-connection-ipc.md](../../../packages/tddy-desktop/docs/changesets/2026-09-05-multi-connection-ipc.md).
