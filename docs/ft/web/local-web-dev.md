# Local web development (`./web-dev`)

## Summary

The repo root script **`./web-dev`** starts **`tddy-daemon`** (RPC backend) and the **`tddy-web`** Vite dev server. The browser app talks to the daemon over the Vite dev proxy (`/rpc` → daemon HTTP port).

**Tddy Desktop (Electrobun):** **`bun run desktop:dev`** runs Vite plus the desktop shell. The embedded daemon defaults to repo-root **`dev.desktop.yaml`** when **`TDDY_DAEMON_CONFIG`** is unset (same shape as **`dev.daemon.yaml`**). Repo-root **`.env`** is loaded first (same “do not override existing env” rule as **`./web-dev`**); daemon env overrides still apply. See [packages/tddy-desktop/README.md](../../../packages/tddy-desktop/README.md).

## Hot reload (HMR) — use the Vite URL

`./web-dev` runs **two HTTP servers**:

| Port (defaults) | Process | What it serves |
|-----------------|---------|----------------|
| **`VITE_PORT`** (default **5173**) | Vite | Dev modules, **HMR**, proxies `/rpc` and `/api` to the daemon |
| **`listen.web_port`** in daemon YAML (default **8899**) | `tddy-daemon` | **`web_bundle_path`** — usually `packages/tddy-web/dist` from the **last `bun run build`** |

**For live edits to React/TS in `packages/tddy-web`, open the Vite URL** (e.g. `http://127.0.0.1:5173`). If you open the daemon URL instead (e.g. `http://127.0.0.1:8899`), the browser loads the **production bundle** from `dist/`; changes to source files will not appear until you rebuild, and there is **no** Vite HMR.

The dev overlay **`HMR: N`** (bottom-left) only increments when the Vite client is active; if you never see it, you are not on the dev server.

## Flow

1. **Daemon binary**: `target/debug/tddy-daemon` when present, otherwise `target/release/tddy-daemon`. Build with `cargo build -p tddy-daemon` when the binary is missing.
2. **Config file**: `DAEMON_CONFIG` selects the YAML path; when unset, the script uses **`dev.daemon.yaml`** at the repo root. The script variable holding that path is named `CONFIG` in the shell; the generic `CONFIG` key from `.env` does **not** supply this path—set **`DAEMON_CONFIG`** for a custom file. That YAML defines **`allowed_tools`** (connection screen **Tool** dropdown via **`ListTools`**) and **`allowed_agents`** (connection screen **Backend** dropdown via **`ListAgents`**); see [Web terminal — Connection screen](web-terminal.md#daemon-mode-connection-screen-project-centric).
3. **Temp config**: A copy of the YAML is written to a temp file with `CURRENT_USER` replaced by the login name (`sed` with `|` delimiters). The daemon receives `-c` pointing at that file.
4. **Extra CLI**: Arguments after the script name pass through to `tddy-daemon`. If both the script and the user pass `-c`, the daemon receives two `-c` arguments; the effective config follows daemon rules. **`DAEMON_PORT`** for the Vite proxy comes from the YAML file selected in step 2 (the `grep`/`awk` port line), not from a second `-c` on the command line.
5. **Vite**: After the daemon answers HTTP **200** on **`GET /api/config`** (not `GET /` — the static bundle under **`web_bundle_path`** may be missing until you run **`bun run build`**), the script starts Vite under `./dev` with `DAEMON_PORT` set for the proxy.

## Environment (common)

| Variable | Role |
|----------|------|
| `DAEMON_CONFIG` | Path to daemon YAML (default `dev.daemon.yaml` at repo root) |
| `WEB_HOST` | **Bind address** for Vite only (default `127.0.0.1`; `0.0.0.0` = all interfaces). Not the URL you type in the browser. |
| `VITE_PORT` | Vite port (default `5173`) |
| `VITE_URL` | **Public app origin** — OAuth, `import.meta.env.VITE_URL`, and what you open in the browser. Default `http://127.0.0.1:<VITE_PORT>`. For LAN: `VITE_URL=http://<your-lan-ip>:5173` (same host/port you use to load the app). `web-dev` sets `WEB_PUBLIC_URL=$VITE_URL` for `tddy-daemon`; do not point `WEB_PUBLIC_URL` at the daemon port (`:8899`) for frontend dev. |
| LiveKit, GitHub, Telegram | See `web-dev` script header (`LIVEKIT_*`, `GITHUB_*`, `TDDY_TELEGRAM_*`); Telegram details in [telegram-notifications.md](../daemon/telegram-notifications.md) |

## Port cleanup

Before starting, the script sends **`fuser -k -9`** on the daemon and Vite TCP ports. On a machine where those ports are shared, free them first or expect processes listening there to be signalled.

## Automated checks

Static contract tests live in **`packages/tddy-e2e`** (`web_dev_contract` module and `tests/web_dev_script.rs`). Run:

```bash
./dev cargo test -p tddy-e2e web_dev --no-fail-fast -- --test-threads=1
```

## Related

- [Web workspace setup](web-workspace-setup.md) — Bun / `tddy-web` package layout
- [Web terminal](web-terminal.md) — Browser terminal over RPC / LiveKit
- [Coder changelog](../coder/changelog.md) — Release notes for the broader Coder area
