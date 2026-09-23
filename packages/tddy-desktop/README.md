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

Where the configuration comes from **depends on the build profile**, and the two have no fallback
between them.

**A release build** — what `./install --desktop` installs — reads **`~/.tddy/desktop.yaml`** and
nothing else: no `TDDY_DAEMON_CONFIG`, no repo-root `dev.desktop.yaml`, no walk up the directory
tree. An installed `.app` launched from the Dock has `/` for a working directory, so anything it
found that way it would have found by accident. The file absent is a startup failure that names
`./install --desktop`; `~/.tddy` is also the workspace root the application moves into.

**A debug build** — `./desktop-dev`, `cargo run` — resolves from the checkout:

1. **Workspace root**: **`TDDY_WORKSPACE_ROOT`**, else the nearest ancestor of the working
   directory (or of the executable) holding `dev.desktop.yaml`, or `Cargo.toml` next to
   `packages/tddy-desktop/package.json`. The application moves into it, so relative paths in the
   YAML mean what they mean for `./web-dev`.
2. **Config**: **`TDDY_DAEMON_CONFIG`**, else repo-root **`dev.desktop.yaml`**. `CURRENT_USER` in
   the YAML is substituted with the OS user, as `./web-dev` does. Neither found is a startup
   failure, not a default.

Both profiles then apply the workspace root's **`.env`** without replacing anything already
exported — the same rule as `./web-dev`.

**`listen.web_port` is required** whichever profile is in play, although this application serves no
HTTP: `runtime::build` refuses to assemble a daemon without it, and here the value names the loopback
port a GitHub sign-in comes back on — `src-tauri/src/oauth_callback.rs` opens a one-path
`/auth/callback` listener on 127.0.0.1 for the duration of a sign-in and closes it again.
`github.redirect_uri` is derived from that port rather than read from the config.

**Sessions need an identity**, and it is two blocks at once: `github:` (without it
`build_auth_entries` returns no session-user resolver, and every session service is assembled behind
one) and `users:` (which OS user a login runs as, with no fallback). No `livekit:` block is needed to
sign in: the daemon signs session tokens with an Ed25519 key it generates into `auth_storage` on first
boot. What `./install --desktop` renders leaves both unset, so a fresh install starts onto its
settings and offers no sessions until they are filled in — see
[config-resolution-and-install.md](docs/config-resolution-and-install.md).

### UI ↔ daemon

Three Tauri commands carry `rpc_envelope` frames as **raw bytes**:

| Command | Arguments | Meaning |
|---------|-----------|---------|
| `tddy_rpc_connect` | `{ channel, clientEpoch, target }` | Open a connection to `target`, with this `Channel<ArrayBuffer>` as its response channel and `clientEpoch` as its identity; every other connection keeps serving, and a `clientEpoch` already in use is refused |
| `tddy_rpc_send` | the encoded `RpcRequest` frame as the invoke body | One request frame, routed by the epoch it carries |
| `tddy_rpc_disconnect` | `{ clientEpoch }` | Release that one connection |

A page holds several at once — the daemon, plus one per attached session — so a connection has a
lifetime of its own rather than the page's, which is what `tddy_rpc_disconnect` exists for. The
page's connections are reaped as a replacing page commits, since a page that has gone can no longer
release what it opened.

The host side is **`tddy-tauri-rpc`** (`MultiConnectionHost`), which knows nothing about Tauri: this
crate supplies the `FrameSink` over `tauri::ipc::Channel` and the `RosterResolver` that turns a
target into a service. The browser side is **`tddy-tauri-web`**'s `thisPagesIpcHost()`, which holds
one bridge per target.

### Window

The dashboard loads from **`VITE_URL`** when set, otherwise from the built `tddy-web` bundle over
Tauri's asset protocol (`frontendDist` → `../../tddy-web/dist`). Links that leave the dashboard
open in the operator's browser through **`tauri-plugin-opener`**. Closing the window kills the
daemon's cli sessions and aborts its runtime tasks, as the binary does on `SIGTERM`.

## Documentation

- Product: [docs/ft/desktop/](../../docs/ft/desktop/)
- Changesets: [docs/changesets/](./docs/changesets/)
