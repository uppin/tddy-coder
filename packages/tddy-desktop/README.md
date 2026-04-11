# tddy-desktop

Native **Electrobun** shell for Tddy: embedded **`tddy-web`**, local **TCP** listener for the Codex OAuth callback, and **LiveKit** **`LoopbackTunnelService`** relay to **`tddy-coder`**.

## Quick start

```bash
# From repo root (nix dev shell)
bun install
bun run --cwd packages/tddy-livekit-web build
bun run desktop:dev
```

`desktop:dev` starts **`tddy-web`**’s Vite dev server and opens Electrobun with `VITE_URL` set automatically.

- **Dev UI (manual)**: run `bun run --filter tddy-web dev`, then `VITE_URL=http://localhost:5173 bun run --filter tddy-desktop dev` (override `VITE_PORT` / `VITE_URL` if you use a non-default port).
- **Production UI**: copy `packages/tddy-web/dist/*` into `resources/web/` before `electrobun build`.
- **OAuth relay** (optional): set `TDDY_RPC_BASE` (e.g. `http://127.0.0.1:8899/rpc`), `TDDY_LIVEKIT_URL`, and `TDDY_LIVEKIT_ROOM` to match your daemon session.

### Embedded `tddy-daemon` (macOS)

The main process can start **`tddy-daemon`** for you so the webview can reach **`/api/config`** and Connect-RPC on the daemon port without a separate terminal.

1. **Config:** If **`TDDY_DAEMON_CONFIG`** is unset, the app uses repo-root **`dev.desktop.yaml`** when that file exists (a dev profile aligned with **`dev.daemon.yaml`** / `./web-dev`). Override with **`TDDY_DAEMON_CONFIG`** for another YAML.
2. **`.env`:** Repo-root **`.env`** is loaded before spawn (variables already set in the environment are **not** replaced). **`tddy-daemon`** then applies its usual env overrides (**`LIVEKIT_URL`**, **`LIVEKIT_API_KEY`**, **`WEB_HOST`**, GitHub vars, etc.) — same as running the daemon manually after sourcing `.env`.
3. **`bun run desktop:dev`** also loads **`.env`** and sets **`TDDY_DAEMON_CONFIG`** to **`dev.desktop.yaml`** when unset, so the Electrobun child inherits it.
4. Ensure a **`tddy-daemon` binary** is available, in order:
   - **`TDDY_DAEMON_BINARY`** (explicit path), or
   - **`packages/tddy-desktop/resources/bin/tddy-daemon`** (from `bun run build-daemon` or `electrobun build` **prebuild**), or
   - **`target/release/tddy-daemon`** or **`target/debug/tddy-daemon`** at the **repo root** (typical after `cargo build -p tddy-daemon`).

The daemon process **`cwd`** is the **repo root** so paths in YAML (**`web_bundle_path`**, **`allowed_tools`**) match **`dev.daemon.yaml`** / **`dev.desktop.yaml`**.

Run **`bun run build-daemon`** from **`packages/tddy-desktop`** (or **`cargo build --release -p tddy-daemon`** from the repo root) before **`electrobun dev`** if you rely on the bundled path.

**`electrobun build`** runs **`prebuild`**, which builds **`tddy-livekit-web`**, then **`cargo build --release -p tddy-daemon`** and copies the binary into **`resources/bin/`** (gitignored). The binary is included in the app bundle via **`electrobun.config.ts`** `build.copy`.

Quitting the desktop sends **SIGTERM** / **beforeExit** cleanup and kills the child daemon. This is a **dev convenience**: a production install may still use a system **`tddy-daemon`** (e.g. launchd) with different users and permissions; YAML must match how you run tools.

## Architecture

Main process (Bun) may spawn **`tddy-daemon`**, opens a `BrowserWindow`, and optionally runs **`installLiveKitOAuthRelay`** with an injected **`startOAuthTcpTunnel`** to join the room, watch **`daemon-*`** metadata, accept loopback TCP for the browser callback, and tunnel bytes over **`loopback_tunnel.LoopbackTunnelService.StreamBytes`** on the LiveKit data-channel RPC envelope.

## Documentation

- Product: [docs/ft/desktop/tddy-desktop-electrobun.md](../../docs/ft/desktop/tddy-desktop-electrobun.md)
- Changesets: [docs/changesets.md](./docs/changesets.md)
