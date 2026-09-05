# Addressed webview IPC connections

How the desktop app's UI↔daemon bridge carries many concurrent connections, and why each piece of
it is shaped the way it is. The product-level description is
[tddy-desktop-tauri.md](../../../docs/ft/desktop/tddy-desktop-tauri.md); this is the implementation.

Three packages are involved, and the split between them is deliberate:

| Package | Owns |
|---|---|
| `tddy-tauri-rpc` | `MultiConnectionHost`, `ConnectionTarget`, `RosterResolver`, `ConnectError`, `FrameError`. Knows nothing about Tauri and nothing about what a target means. |
| `tddy-desktop/src-tauri` | The three commands, the `FrameSink` over `tauri::ipc::Channel`, `DaemonRosters`, the wire spelling of a target, and the page-load reap. |
| `tddy-tauri-web` | The page's per-target bridge registry, the bridge's own epoch and lifetime, and the `FramePipe` the envelope transport is built on. |

## The host: a map keyed by the client epoch

`MultiConnectionHost` holds `HashMap<u32, Connection>`, keyed by the epoch the page already stamps
its frames with. Everything else about the single-connection design survived, which is what made the
change a change of container rather than of protocol:

- the engine is already peer-keyed (`webview-{epoch}`) and releases a peer's state as a unit;
- backpressure was already per connection — one bounded queue (`RESPONSE_QUEUE_CAPACITY = 256`) and
  one drain task each;
- the epoch already routes frames, so there is no frame-format change and no version to negotiate.

A `Connection` carries its own `ServerEngine<Arc<dyn RpcService>>`, its peer string, its sink and the
sender half of its response queue. The engine is per connection rather than shared because two
connections may resolve to different rosters, and because releasing one must leave what the others
have in flight untouched. The roster is a trait object because a `RosterResolver` picks it at
runtime — the host cannot name the type it will be handed.

### A connection has an id as well as an epoch

`Connection.id` is a `u64` from an atomic counter, and it exists solely so a departing connection
cannot tear down its own successor. An epoch is a key, not a generation: the map keys on it and the
engine peer derives from it, so neither can distinguish a connection from the next one opened under
the same epoch. `release_departed_connection` therefore removes the entry only if the connection
still under that key has the same id.

### Lock discipline

The map's mutex is shared with every drain task, because a departed page is first noticed by the task
publishing to it — which is therefore where the connection has to be removed. That gives two rules
the host obeys throughout:

- **`connect` holds the lock across the containment check and the insert**, so two connects racing on
  one epoch cannot both find it free.
- **`handle_request_frame` and `disconnect` borrow the map only long enough to take what they need
  and then drop it.** `on_request` runs a handler that may block on a drain task making room in a
  bounded response queue, and that drain task needs the same lock to report its page gone; a release
  performed under the lock would deadlock against the forward it is waiting for.

`handle_request_frame` also yields to the runtime before dispatching. A failed publish is the only
way this host ever learns a page is gone, so a host driven purely by inbound frames would go on
dispatching calls for a page that left; giving the runtime a turn lets responses already produced
reach their sinks first.

### Which refusal a frame gets

`FrameError` is unchanged, and `MultiConnectionHost` narrows the meaning of its variants rather than
adding one. `StaleConnection` names the connection the frame missed, but its `connected` field is a
*single* epoch and a host serving many has no single "the connected one" — so it is reported only
when exactly one connection is open, where naming it is the truth, and `NotConnected` otherwise.

The refusal itself is not optional. Dispatching a frame that names no open connection onto some other
connection would send its answer out on that connection's sink, where it is dropped for the epoch
mismatch, and the caller would wait for an answer that can never arrive.

`ConnectError` is the connect-time counterpart: `NoSuchTarget` when the resolver serves nothing, and
`EpochInUse` when a connection is already registered under that epoch. `EpochInUse` leaves the
incumbent serving — epochs are minted one per connection on the page side, so a collision is the
caller's mistake, and evicting the connection already there would lose its in-flight calls for
someone else's error.

### Release paths

Three of them, and they converge on one `release`: the engine drops the peer's state (which aborts
the forwards still publishing for it) and the sink is closed. Dropping the `Connection` drops the last
sender of its response queue, which is what ends its drain task.

- `disconnect(epoch)` — the page gave one connection back. Idempotent, so a detach racing an unmount
  is harmless.
- `disconnect_all()` — the page is gone. See the reap below.
- `release_departed_connection` — a sink refused a frame, so the page behind that one channel is
  gone. Only that connection is removed; that a page lost one channel says nothing about the rest.

## The desktop's seam

`ipc.rs` is the whole Tauri-facing surface. `build.rs` names the three commands and
`capabilities/default.json` grants them to the `main` window — an application command that is not
named has no permission to grant and is refused at runtime.

**`DaemonRosters` resolves every target to the daemon's own roster.** This is not a placeholder. The
embedded daemon serves session-scoped RPC itself and routes it by what the request names rather than
by the connection it arrived on, so each connection still gets its own epoch, engine, peer and
backpressure and can be released independently, while the roster behind every target is the one
daemon. It is deliberately *not* gated on a live session: `RosterResolver::roster_for` is synchronous
and the only lookup that could say a session is live — `CliSessionManager::get` — is async, so
refusing an unknown session at connect would mean widening a `tddy-tauri-rpc` trait for this host's
convenience. The consequence, which the crate's own docs state: `ConnectError::NoSuchTarget` never
fires on the desktop, and a call naming a session the daemon does not have is answered by the daemon
with the error it answers a served page with.

**`TargetArgument` is a wire type of its own** rather than a `Deserialize` on `ConnectionTarget`.
`tddy-tauri-rpc` has no serde dependency and is not to grow one; how a target is spelt over *this*
host's IPC is this host's business. It is an internally tagged enum — `{ kind: "daemon" }` or
`{ kind: "session", sessionId }` — and needs `rename_all_fields` as well as `rename_all`, because the
container attribute renames variant *names* and nothing inside them.

**`ChannelSink::close` does nothing on purpose.** A Tauri channel has no end-of-stream marker, and the
page that owned one either released it deliberately or went away with its window; dropping the channel
along with the sink is the whole release.

**`tddy_rpc_send` awaits the dispatch.** Each connection's response queue is bounded, so a page that
has stopped reading one channel has that connection's `invoke` promise held rather than being allowed
to pile up work the daemon cannot deliver — and only that connection's.

### The page-load reap

`on_page_load` fires on the *arriving* page, so the ordering is the difficulty rather than the reap
itself. `reap_the_departing_pages_connections` returns early on anything but `PageLoadEvent::Started`
(the commit of the new document, before its scripts are injected and therefore before it can invoke
anything), on any webview but the dashboard, and before the daemon is assembled — which is where the
very first page load lands.

It then runs `disconnect_all` under `block_on`, on that thread. Blocking is the point: a spawned reap
could still be running once the arriving page has begun opening connections of its own, and would take
those with it.

Nothing else can do this. A page that has gone cannot release epochs it no longer remembers, and the
host notices a departed page only lazily, when a response it can no longer deliver is published —
which for an idle connection is never.

## The page side

`thisPagesIpcHost()` returns a module-level registry keyed by `connectionKey(target)`
(`"daemon"` or `"session:{id}"`), because the target union has no identity of its own:
`sessionTarget(id)` twice yields two objects equal in every way that matters and `===` in none. The
registry is what holds the invariant **one bridge per target**, which replaced *one bridge per page*.
There is no exported way to build a bridge outside it — one built outside would be a second
connection to something the page already reaches, under a second epoch nothing would ever release.

A bridge mints its own `clientEpoch` at construction and exposes it read-only; `createTauriTransport`
carries it rather than choosing it. Minted by whoever builds a transport instead, a page building two
transports over one bridge would open two connections to a target it reaches once, the second
stamping frames with an epoch the first registration never named.

Neither command runs while a bridge is merely being built. `openConnection` may run for a component
that never issues a call, so the host is asked for a peer only when one is wanted.

### Two races `close()` has to survive

`close()` is idempotent, drops the registry entry through its `onReleased` callback *before* telling
the host, and resolves `closed` so calls in flight settle instead of hanging.

- **`close` racing a mount.** The bridge keeps the pending registration promise, not just the fact of
  one, and `close` awaits it before invoking `tddy_rpc_disconnect` — otherwise it would return while
  the registration in flight is still creating the very peer it meant to release. A registration the
  host *refused* left no peer behind, so nothing is asked of the host in that case.
- **`connect` landing after `close`.** A `released` flag gates `connect` as well, because keeping the
  pending registration cannot cover this ordering: a registration made after an idempotent `close`
  has already run would create a peer nothing will ever come back for. It returns quietly rather than
  throwing — `closed` resolved as the bridge was released, so the transport above it has already been
  told and settles its calls without waiting on a channel.

`onReleased` drops the registry entry only if the bridge being released is still the one held, so a
target reattached during a release does not have its fresh bridge dropped by its predecessor.

### `daemonTransport.ts`

`tddy-web`'s module-level `thisPagesBridge` singleton is gone. `thisPagesHost().createIpcBridge` is
`thisPagesIpcHost().openConnection(DAEMON_TARGET)`, so the guarantee that a page holds one daemon
connection now comes from the registry, which keys on the target and can express a page holding a
session connection alongside it. `createDefaultDaemonTransport`'s signature, its flavour choice and
its interceptor stack are unchanged.

## `Arc<S>` implements `RpcService`

A resolver hands back `Arc<dyn RpcService>` and a `ServerEngine` needs an `S: RpcService`, so
`tddy-rpc/src/bridge.rs` carries a blanket `impl<S: RpcService + ?Sized> RpcService for Arc<S>` that
delegates every method. Additive, no new dependency, and consistent with `ServiceEntry`, which already
stored rosters this way.

## `WebviewRpcHost` is still there

The single-connection host is unchanged and still exported. It has no production caller — `ipc.rs`
runs `MultiConnectionHost` — but it remains the smaller shape for a host that genuinely serves one
webview, and the crate's tests run the shared call-shape and lifecycle bodies against **both** hosts
through an `against_both_hosts!` macro over a narrow `WebviewHost` trait. The two deliberately part
on what *opening* a connection does: `WebviewRpcHost::connect` abandons the incumbent, and that is
where its own tests live.
