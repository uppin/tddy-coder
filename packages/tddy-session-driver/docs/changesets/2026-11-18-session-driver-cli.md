# 2026-11-18 — A CLI runs one agent session on a local, ssh or Coder target, driven over MCP

**Type:** Feature · `#coder-start` 3/3, PR [#942](https://github.com/uppin/tddy-coder/pull/942)

- New package and binary `tddy-session-driver`: launches one session through `launch_host_pty`
  (local) or `launch_on_target` (ssh, Coder) and serves six MCP tools on stdio — `start_session`,
  `read_terminal`, `send_keys`, `wait_for_text`, `session_status`, `stop_session`.
- stdout carries only MCP frames. A Coder target takes `CODER_URL` / `CODER_SESSION_TOKEN` from the
  environment and is refused before anything runs without them.
- `--binary` / `--env` name the agent's binary and environment on the target, working around the
  target launch's use of the daemon host's agent path and environment (`TMPDIR` included).
- A developer tool: not built by `./release` nor installed by `./install`.

Pinned by the 7 tests of `session_driver_acceptance` and 3 unit tests in `target.rs`. See
[architecture.md](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-18-session-driver-cli.md`](../../../../docs/dev/changesets/2026-11-18-session-driver-cli.md).
