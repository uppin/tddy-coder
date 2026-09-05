# Changeset: optional-livekit-multi-connection-ipc

**Stack:** `optional-livekit` — node 6 of 7, **root** (base `master`)
PR: [#442](https://github.com/uppin/tddy-coder/pull/442)
PRD: [`2026-09-05-optional-livekit-multi-connection-ipc-prd.md`](2026-09-05-optional-livekit-multi-connection-ipc-prd.md)
Discovery: [`2026-09-05-optional-livekit-multi-connection-ipc-initial-discovery.md`](2026-09-05-optional-livekit-multi-connection-ipc-initial-discovery.md)

## State A

- `packages/tddy-tauri-rpc/src/host.rs` (233 lines) — `WebviewRpcHost<S: RpcService>` holds
  `engine: Arc<ServerEngine<S>>` and `connection: Arc<Mutex<Option<Connection>>>`, **one slot**.
  `connect(sink, client_epoch)` abandons whatever was there; `handle_request_frame` refuses a frame
  whose `client_epoch` differs (`FrameError::StaleConnection`); `Connection::peer_for(epoch)` names
  the engine peer `webview-{epoch}`; `drain_responses` publishes per connection with a bounded queue
  (`RESPONSE_QUEUE_CAPACITY = 256`); `release_departed_peer` clears the slot only if it still holds
  that peer.
- `packages/tddy-desktop/src-tauri/src/ipc.rs` — two commands, `tddy_rpc_connect(channel, client_epoch)`
  and `tddy_rpc_send(request)`, over one `RpcState { host: WebviewRpcHost<MultiRpcService> }`. No
  target on either.
- `packages/tddy-tauri-web/src/transport.ts` — `createTauriIpcBridge()` registers one
  `Channel<ArrayBuffer>` via `invoke("tddy_rpc_connect", {channel, clientEpoch})`; `webviewFramePipe`
  turns it into a `FramePipe`; `createTauriTransport` wraps it in `createEnvelopeTransport`. No
  close path — `closed` is `new Promise(() => {})` on the stated reasoning that host and page share
  one lifetime.
- `packages/tddy-web/src/rpc/daemonTransport.ts` — module-level `thisPagesBridge` singleton, with a
  comment explaining that a page opening two bridges would abandon its own first connection.

The engine layer is already peer-based, which is what makes this tractable: `ServerEngine` keys state
by a peer string and `on_peer_disconnected` releases it. Only the host's container is singular.

## State B

- `WebviewRpcHost` holds a **map** of connections keyed by client epoch. Opening one does not disturb
  another; closing one leaves the rest serving; backpressure is per connection.
- Each connection resolves its roster at `connect` time through a `RosterResolver`, given a
  `ConnectionTarget` (`Daemon` | `Session { session_id }`). An unresolvable target is refused at
  connect with a reason.
- A third command `tddy_rpc_disconnect` releases one connection. A page reload reaps every connection
  the previous page owned.
- `tddy-tauri-web` exposes `openConnection(target)` — an independent bridge per call, each with its
  own `Channel` and epoch — plus `close()`.
- `daemonTransport.ts`'s singleton becomes a per-target registry; `createDefaultDaemonTransport()`
  keeps its signature and opens the `Daemon` connection exactly once per page.
- No room, participant or LiveKit-shaped identity appears anywhere in this node's code.

## Responsibility

- `packages/tddy-tauri-rpc`: the connection map, `ConnectionTarget`, `RosterResolver`, per-connection
  lifecycle (connect / disconnect / departure / page-reload reaping), and `FrameError`'s narrowed
  meanings.
- `packages/tddy-desktop/src-tauri/src/ipc.rs`: the target on `tddy_rpc_connect`, the new
  `tddy_rpc_disconnect`, the `RosterResolver` implementation that maps a session id to the daemon's
  session-scoped roster, and the capability entry in `capabilities/default.json`.
- `packages/tddy-tauri-web`: `openConnection(target)`, per-connection bridges, `close()`.
- `packages/tddy-web/src/rpc/daemonTransport.ts`: the per-target bridge registry replacing the
  singleton, with `createDefaultDaemonTransport()` unchanged in behaviour.
- Rust tests in `tddy-tauri-rpc` (extending `rpc_over_webview_ipc.rs` and
  `webview_connection_lifecycle.rs`) and bun tests in `tddy-tauri-web` (`transport.test.ts`).

## Boundaries

- Does **not** define or touch `tddy-web`'s connection model, host directory, session abstraction or
  capability gating (nodes 1–5). This node ships a transport, not a provider.
- Does **not** register anything with `tddy-web`'s provider registry, and does not make the desktop
  app use the new connections for sessions. Node 7 owns that.
- Does **not** change the frame format, `tddy-rpc-web`'s envelope layer, `MultiRpcService`, or the
  `ServerEngine`'s peer contract.
- Does **not** change LiveKit anything, in `tddy-web` or in the daemon.
- Does **not** change `createDefaultDaemonTransport`'s signature or the interceptor stack (traffic
  meter + auth gate) either flavour carries.
- Does **not** introduce a string-typed target — `ConnectionTarget` stays a closed enum precisely so
  LiveKit identity strings cannot leak across.
- Adds **no npm dependency and no Rust dependency**.

## Dependencies

This is a **root** node: it has no parent PRs and consumes nothing from the stack. It branches off
`master` and can merge on its own, in parallel with nodes 1–5.

Node 7 depends on it, and also on node 4 — so this node's push is what unblocks half of node 7.

## Draft PR contract

Lands first, so node 7 can branch off a real ref and compile against real signatures:

1. `packages/tddy-tauri-rpc/src/lib.rs` — `ConnectionTarget`, `RosterResolver`, and
   `WebviewRpcHost`'s new `connect` / `disconnect` signatures.
2. `packages/tddy-tauri-web/src/transport.ts` — `WebviewIpcHost.openConnection(target)` and
   `WebviewIpcBridge.close()` signatures.
3. Failing Rust tests: two concurrent connections serve independently; closing one leaves the other
   serving; an unresolvable target is refused at connect; a frame for an unknown epoch is refused;
   a page reload reaps the previous page's connections.
4. Failing bun tests in `tddy-tauri-web`: `openConnection` yields independent bridges with distinct
   epochs; `close()` releases the host side; a send on a closed bridge reports the connection gone.

Implementation lands in the same PR under `/green`. **Not a merge candidate on the contract alone.**

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `packages/tddy-tauri-rpc/tests/concurrent_webview_connections.rs`
- [x] Run acceptance tests (verify they fail) — 8/8 on `MultiConnectionHost`
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `packages/tddy-tauri-web/src/multiConnection.test.ts`
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

### Baseline and red status at the contract commit

| Check | Result |
|---|---|
| `cargo build -p tddy-tauri-rpc` | clean on the published surface |
| `cargo clippy -p tddy-tauri-rpc --all-targets -- -D warnings` | clean |
| `cargo fmt --all --check` | clean |
| `cargo check -p tddy-desktop` | clean — the new `tddy_rpc_disconnect` command compiles and is registered in `lib.rs` |
| `cargo test -p tddy-tauri-rpc --test rpc_over_webview_ipc --test webview_connection_lifecycle` | **14 pass, 0 fail** — no regression to the single-connection host |
| `cargo test -p tddy-tauri-rpc --test concurrent_webview_connections` | **8 tests, 8 failing** |
| `cd packages/tddy-tauri-web && bun test` | 7 pass (pre-existing), **5 failing** (this node's) |

Every failure is on this node's own `TODO(multi-connection-ipc)` bodies — `MultiConnectionHost`'s
methods, `thisPagesIpcHost`, `WebviewIpcBridge.close`, `tddy_rpc_disconnect`. This is a root node, so
none is on a parent's surface.

The existing 14 tests passing matters as much as the 8 failing: `WebviewRpcHost` is untouched and
still serves the single-connection path, so nothing that works today depends on this node landing.

### Commands

Scoped to the packages this node touches:

```bash
cargo build -p tddy-tauri-rpc
cargo clippy -p tddy-tauri-rpc -- -D warnings
cargo test -p tddy-tauri-rpc
./dev bun run --filter tddy-tauri-web test
./dev bun run --filter tddy-web test:unit          # daemonTransport registry
./desktop-dev                                      # manual: tddy-desktop is NOT in the CI gate
```

`tddy-desktop`'s Tauri crate is outside the four required checks, so the desktop half needs a
reported local run — a green PR does not cover it. Installs via `bun run local-registry-install`; the
public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-desktop-ipc-host.md`](2026-09-05-optional-livekit-desktop-ipc-host.md)
  — branch `feature/optional-livekit/desktop-ipc-host`
