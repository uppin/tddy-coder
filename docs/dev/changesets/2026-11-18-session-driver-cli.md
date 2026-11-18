# 2026-11-18 — A CLI runs one agent session on a local, ssh or Coder target, driven over MCP

**Type:** Feature · `#coder-start` 3/3, PR [#942](https://github.com/uppin/tddy-coder/pull/942)
**Packages:** `tddy-session-driver` (new), `tddy-tool-engine`, `tddy-agent-launch`

Checking that an agent session really starts on a target took temporary Rust tests, written, run
against `mantasi/remote-dev` and deleted for each of #939 and #941. `tddy-session-driver` replaces
them: it starts **one** session through the daemon's own launch code on `--target local`,
`ssh:<alias>` or `coder:<owner>/<workspace>[.<agent>]`, and serves an MCP server on stdin/stdout
whose six tools (`start_session`, `read_terminal`, `send_keys`, `wait_for_text`, `session_status`,
`stop_session`) mean what the orchestrator's terminal tools mean. Usage and answers:
[`packages/tddy-session-driver/docs/architecture.md`](../../../packages/tddy-session-driver/docs/architecture.md).

- `tddy-tool-engine`: the private `grid_answer` is now `pub fn terminal_grid_answer`, taking a
  `GridRegion`, so the driver's `read_terminal` answers exactly what `ReadSessionTerminal` does.
- `tddy-agent-launch`: `REMOTE_AGENT_PID_FILE` is `pub` and re-exported from `remote_agent_launch`;
  `stop_session` kills a remote agent by it, since dropping the local client leaves it running.
- Not shipped: `./release`, `./install` and `scripts/dev-runtime-binaries.sh` are unchanged.

**Pinned by** `session_driver_acceptance` (7 tests, the compiled binary over the real JSON-RPC wire,
failing on any non-frame stdout line; the remote-stop test fails with the pid kill disabled) and 3
unit tests in `target.rs`.

**Run against the real deployment** (`mantasi/remote-dev`, 2026-11-18), with `--binary
/home/coder/.local/bin/claude --env HOME=… --env PATH=… --env TMPDIR=/tmp`: started after 44 s,
Claude completed a turn reporting the workspace's hostname and the fresh worktree, and
`stop_session` left no `claude` on the VM. **No folder-trust dialog appeared**: trust is kept per
repository and an earlier run had already trusted it, so answering that dialog through the driver
was not exercised live. Without `--env TMPDIR=/tmp` Claude died `EACCES … mkdir '/var/folders'` —
the daemon host's `TMPDIR` reaching the target, now recorded on the open backlog entry
`docs/dev/todo/2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md`.

Left open: that entry (worked around by `--binary` / `--env`, not fixed),
`2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt.md` (nothing answers dialogs
automatically) and `2026-11-16-remote-shell-test-fixtures-are-duplicated.md` (this suite adds an
eighth fixture copy, recorded there).
