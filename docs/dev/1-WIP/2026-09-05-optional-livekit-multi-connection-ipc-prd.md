# PRD: concurrent addressed webview IPC connections

**Stack:** `optional-livekit` — node 6 of 7 (`multi-connection-ipc`), root
**Target PRD on wrap:** [`docs/ft/desktop/tddy-desktop-tauri.md`](../../ft/desktop/tddy-desktop-tauri.md)
**Date:** 2026-09-05

## Problem

The desktop app's IPC bridge is **single-connection by construction**, at all three layers:

- `tddy-tauri-rpc` — `WebviewRpcHost` "hosts `S` for a single webview at a time". It holds
  `connection: Arc<Mutex<Option<Connection>>>` — one slot. `connect()` abandons whatever was there,
  and a frame whose `client_epoch` does not match the connected one is refused with
  `FrameError::StaleConnection`.
- `tddy-desktop` — `ipc.rs` exposes exactly two commands, `tddy_rpc_connect` and `tddy_rpc_send`,
  over one `RpcState` holding one `WebviewRpcHost<MultiRpcService>`. There is no address on either.
- `tddy-web` — `daemonTransport.ts` keeps a module-level `thisPagesBridge` singleton, deliberately:
  "registering a response channel *abandons the previous one*", so a page that opened two would
  abandon its own first connection.

So a page gets one connection, and that connection reaches one roster: the daemon's. There is no way
to address a session. Over LiveKit a session is a **participant** you target inside a room; over IPC
there is no equivalent, which is why session-scoped work on desktop falls back to the daemon.

## What this PR delivers

Many concurrent, independently addressed IPC connections per page, each reaching the roster its
target names — with nothing LiveKit-shaped anywhere in it.

### The shape

**The client epoch becomes the connection id.** `mintClientEpoch()` already mints one per transport;
today a page happens to build one transport. Keying the host's connection map by epoch makes
multi-connection a change of container, not of protocol — no frame format change, and
`StaleConnection` keeps its meaning, narrowed from "a page that was replaced" to "a connection this
host does not have".

```rust
// tddy-tauri-rpc
/// Resolves a connection's target to the roster that serves it.
pub trait RosterResolver: Send + Sync + 'static {
    fn roster_for(&self, target: &ConnectionTarget) -> Option<Arc<dyn RpcService>>;
}

/// What a page's connection asked to reach. No rooms, no participants.
pub enum ConnectionTarget {
    Daemon,
    Session { session_id: String },
}
```

```ts
// tddy-tauri-web
interface WebviewIpcHost {
  openConnection(target: ConnectionTarget): WebviewIpcBridge;   // each with its own channel + epoch
}
interface WebviewIpcBridge {
  /* connect / send / closed, as today */
  close(): Promise<void>;                                        // releases the host side
}
```

### Acceptance criteria

1. `WebviewRpcHost` holds a **map** of connections keyed by client epoch, not one slot. Opening a
   second connection does not disturb the first, and calls in flight on either are unaffected.
2. Each connection resolves its roster through a `RosterResolver` at `connect` time. An unresolvable
   target is refused **at connect**, with a reason, not by silently answering nothing.
3. `ConnectionTarget` names `Daemon` and `Session { session_id }` and nothing else. No room, no
   participant, no identity string of LiveKit's shape appears in `tddy-tauri-rpc`, `tddy-tauri-web`
   or `ipc.rs`.
4. A third command, `tddy_rpc_disconnect`, releases one connection: the engine drops that peer's
   state, in-flight forwards abort, and the sink is closed. Closing one connection leaves the others
   serving.
5. `tddy_rpc_connect` takes the target; `tddy_rpc_send` is unchanged on the wire — the frame's
   `client_epoch` already routes it.
6. **A page reload reaps every connection the previous page owned.** Today one slot made that
   automatic; with a map it must be explicit, and a leaked per-session connection per reload is the
   failure this criterion exists to prevent.
7. `tddy-tauri-web` exposes `openConnection(target)`, each call yielding an independent bridge with
   its own `Channel` and epoch, plus `close()`. The `thisPagesBridge` singleton in
   `daemonTransport.ts` becomes a registry keyed by target, so the daemon connection is still opened
   exactly once per page.
8. `createDefaultDaemonTransport()` keeps its present behaviour and signature — it now opens the
   `Daemon`-targeted connection through the registry.
9. Backpressure is per connection: a page that stops reading one channel does not stall the others.
   The bounded `RESPONSE_QUEUE_CAPACITY` applies per connection.
10. Existing tests in `tddy-tauri-rpc` (`rpc_over_webview_ipc.rs`,
    `webview_connection_lifecycle.rs`) and `tddy-tauri-web` (`transport.test.ts`) pass, extended for
    the multi-connection cases.

### Non-goals

Nothing in `tddy-web`'s connection model, directory, session abstraction or capability gating — this
node delivers the transport, node 7 registers a provider over it. No LiveKit change. See the
changeset's `## Boundaries`.

## Why this shape

- **The epoch is already a connection id.** It is minted per transport and already routes frames.
  Re-using it avoids a protocol change and keeps `StaleConnection`'s diagnostic value.
- **The resolver, not the host, knows what a target means.** `tddy-tauri-rpc` stays a generic
  webview-RPC host; only `tddy-desktop` knows that a session id maps to the daemon's session-scoped
  roster. That is what keeps the crate reusable and the layering honest.
- **`ConnectionTarget` is deliberately closed.** An open string target would invite LiveKit identity
  strings (`daemon-<instance>-<session>`) to leak across, which is precisely what this stack exists
  to prevent.
- **Disconnect must be explicit.** Sessions come and go far more often than pages do. Without a
  release path, every attach leaks a host-side peer.

## Constraints

- **Zero new npm dependencies** and no new Rust dependencies (no public npm registry;
  `bun run local-registry-install`).
- **`tddy-desktop` is not in the CI gate** (`docs/dev/guides/ci.md`), so the desktop half needs local
  verification evidence — a `./desktop-dev` run — reported explicitly, not inferred from a green PR.
- Frame format is unchanged; `tddy-rpc-web`'s envelope layer is untouched.

## Successor PRs

- `feature/optional-livekit/desktop-ipc-host` — the connection provider registered over this
  transport.
