# tddy-desktop

**Tddy Desktop**: a **Tauri** application whose Rust process *is* a **`tddy-daemon`**. One process,
one service roster, and **no TCP listener** — the dashboard reaches the daemon over the webview's
IPC bridge, which no other process on the machine can address.

## Quick start

```bash
# From the repo root (nix dev shell)
bun install
bun run desktop:dev
```

`desktop:dev` starts **`tddy-web`**'s Vite dev server and launches the app with `VITE_URL` set. For
a bundle instead of a dev server: `bun run --filter tddy-web build && bun run --filter tddy-desktop build`.

## How it is put together

| Piece | Where |
|-------|-------|
| Application entry, window, shutdown | `src-tauri/src/lib.rs` |
| Daemon configuration resolution | `src-tauri/src/config_source.rs` |
| The two IPC commands | `src-tauri/src/ipc.rs` |
| Tauri configuration and icons | `src-tauri/tauri.conf.json`, `src-tauri/icons/` |

The Rust crate is a workspace member (`tddy-desktop`), so `cargo build --workspace`,
`cargo clippy --workspace` and `cargo test --workspace` all cover it.

### The daemon it hosts

The application assembles the daemon with **`tddy_daemon::runtime::build`** under
**`RuntimeOptions::for_embedded()`** — the same roster the **`tddy-daemon`** binary assembles,
minus the HTTP listener and systemd socket activation. The spawn worker is forked *before* any
async runtime exists, because `fork` from a multi-threaded process can deadlock.

1. **Workspace root**: **`TDDY_WORKSPACE_ROOT`**, else the nearest ancestor of the working
   directory (or of the executable) holding `dev.desktop.yaml`, or `Cargo.toml` next to
   `packages/tddy-desktop/package.json`. The application moves into it, so relative paths in the
   YAML mean what they mean for `./web-dev`.
2. **`.env`**: repo-root **`.env`** is applied without replacing anything already exported — the
   same rule as `./web-dev`.
3. **Config**: **`TDDY_DAEMON_CONFIG`**, else repo-root **`dev.desktop.yaml`**. `CURRENT_USER` in
   the YAML is substituted with the OS user, as `./web-dev` does. Neither found is a startup
   failure, not a default.

### UI ↔ daemon

Two Tauri commands carry `rpc_envelope` frames as **raw bytes**:

| Command | Arguments | Meaning |
|---------|-----------|---------|
| `tddy_rpc_connect` | `{ channel, clientEpoch }` | Register this page's `Channel<ArrayBuffer>` as its response channel; the host abandons every stream the previous page opened |
| `tddy_rpc_send` | the encoded `RpcRequest` frame as the invoke body | One request frame |

The host side is **`tddy-tauri-rpc`** (`WebviewRpcHost`), which knows nothing about Tauri: this
crate supplies the `FrameSink` over `tauri::ipc::Channel`. The browser side is
**`tddy-tauri-web`**'s `createTauriIpcBridge()`.

### Window

The dashboard loads from **`VITE_URL`** when set, otherwise from the built `tddy-web` bundle over
Tauri's asset protocol (`frontendDist` → `../../tddy-web/dist`). Links that leave the dashboard
open in the operator's browser through **`tauri-plugin-opener`**. Closing the window kills the
daemon's cli sessions and aborts its runtime tasks, as the binary does on `SIGTERM`.

## Documentation

- Product: [docs/ft/desktop/](../../docs/ft/desktop/)
- Changesets: [docs/changesets/](./docs/changesets/)
