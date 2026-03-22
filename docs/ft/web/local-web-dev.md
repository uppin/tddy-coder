# Local web development (`./web-dev`)

## Summary

The repo root script **`./web-dev`** starts **`tddy-daemon`** (RPC backend) and the **`tddy-web`** Vite dev server. The browser app talks to the daemon over the Vite dev proxy (`/rpc` → daemon HTTP port).

## Flow

1. **Daemon binary**: `target/debug/tddy-daemon` when present, otherwise `target/release/tddy-daemon`. Build with `cargo build -p tddy-daemon` when the binary is missing.
2. **Config file**: `DAEMON_CONFIG` selects the YAML path; when unset, the script uses **`dev.daemon.yaml`** at the repo root. The script variable holding that path is named `CONFIG` in the shell; the generic `CONFIG` key from `.env` does **not** supply this path—set **`DAEMON_CONFIG`** for a custom file.
3. **Temp config**: A copy of the YAML is written to a temp file with `CURRENT_USER` replaced by the login name (`sed` with `|` delimiters). The daemon receives `-c` pointing at that file.
4. **Extra CLI**: Arguments after the script name pass through to `tddy-daemon`. If both the script and the user pass `-c`, the daemon receives two `-c` arguments; the effective config follows daemon rules. **`DAEMON_PORT`** for the Vite proxy comes from the YAML file selected in step 2 (the `grep`/`awk` port line), not from a second `-c` on the command line.
5. **Vite**: After the daemon answers HTTP 200 on its port, the script starts Vite under `./dev` with `DAEMON_PORT` set for the proxy.

## Environment (common)

| Variable | Role |
|----------|------|
| `DAEMON_CONFIG` | Path to daemon YAML (default `dev.daemon.yaml` at repo root) |
| `WEB_HOST` | Vite bind address (default `127.0.0.1`) |
| `VITE_PORT` | Vite port (default `5173`) |
| `WEB_PUBLIC_URL`, LiveKit, GitHub | Documented in the script header for daemon OAuth and integrations |

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
