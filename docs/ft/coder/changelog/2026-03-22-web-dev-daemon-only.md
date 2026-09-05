# 2026-03-22 — web-dev: daemon-only

- **Local web stack**: `./web-dev` runs **`tddy-daemon`** with **`dev.daemon.yaml`** at the repo root when **`DAEMON_CONFIG`** is unset; **`DAEMON_CONFIG`** selects another YAML path. The script resolves the debug or release daemon binary, writes a temp YAML with **`CURRENT_USER`** substituted, passes through CLI arguments, derives **`DAEMON_PORT`** from that YAML for the Vite **`/rpc`** proxy, and starts **`packages/tddy-web`** via Vite under **`./dev`**.
- **Daemon config vs `.env` `CONFIG`**: The daemon YAML path comes from **`DAEMON_CONFIG`** / **`dev.daemon.yaml`**, not from the generic **`CONFIG`** variable that `.env` may set for other tools.
- **Docs**: [Local web dev](../../web/local-web-dev.md) describes the flow, env vars, and contract tests.
