# 2026-09-05 — Three commands, and a connection per thing the page reaches

**Type:** Architecture

`ipc.rs` runs a `MultiConnectionHost` instead of a `WebviewRpcHost`, and the UI↔daemon contract grows
from two commands to three:

| Command | Arguments | Meaning |
|---|---|---|
| `tddy_rpc_connect` | `{ channel, clientEpoch, target }` | open one connection to `target`, with `channel` as its response channel and `clientEpoch` as its identity |
| `tddy_rpc_send` | the encoded `RpcRequest` frame as the raw invoke body | one request frame, routed by the epoch it carries |
| `tddy_rpc_disconnect` | `{ clientEpoch }` | release that one connection |

`build.rs` names all three and `capabilities/default.json` grants them to the `main` window; an
application command that is not named has no permission to grant and is refused at runtime.

`DaemonRosters` is this crate's `RosterResolver`, and it resolves **every** target to the daemon's own
roster. Not a placeholder: the embedded daemon serves session-scoped RPC itself and routes it by what
the request names rather than by the connection it arrived on, so each connection still gets its own
epoch, engine, peer and backpressure and is released independently, while the roster behind every
target is the one daemon. It is deliberately not gated on a live session — `roster_for` is
synchronous and `CliSessionManager::get` is async — so `ConnectError::NoSuchTarget` never fires here
and a call naming a session the daemon does not have is answered by the daemon with the error it
answers a served page with.

`TargetArgument` is a wire type of its own rather than a `Deserialize` on `tddy-tauri-rpc`'s
`ConnectionTarget`: that crate has no serde dependency and is not to grow one, and how a target is
spelt over this host's IPC is this host's business. It is internally tagged
(`{ kind: "daemon" }` / `{ kind: "session", sessionId }`) and carries `rename_all_fields` as well as
`rename_all`, because the container attribute renames variant names and nothing inside them.

`lib.rs` reaps a departing page's connections on `on_page_load`. The difficulty is ordering, since the
event fires on the *arriving* page: the reap runs at `PageLoadEvent::Started` — the commit of the new
document, before its scripts are injected — scoped to the dashboard window, and under `block_on` on
that thread, because a spawned reap could still be running once the new page has begun opening
connections of its own. Nothing else can do it: a page that has gone cannot release epochs it no
longer remembers, and the host notices a departed page only when a response it can no longer deliver
is published, which for an idle connection is never.

`serde` becomes a direct dependency (and `serde_json` a dev-dependency) because a Tauri command
argument must be `Deserialize`. Both were already linked through `tauri`, so no crate enters the tree.

`./desktop-dev` resolved `PROJECT_ROOT` one level above the repo, which pointed
`TDDY_WORKSPACE_ROOT`, `TDDY_DATA_DIR` and the `dev.desktop.yaml` lookup at the checkout's parent.
The script lives at the repo root, so the root is its own directory — the same rule `./web-dev`
follows.

Implementation notes: [addressed webview IPC connections](../webview-ipc-connections.md). Product:
[tddy-desktop-tauri.md](../../../../docs/ft/desktop/tddy-desktop-tauri.md).
